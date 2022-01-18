//! App
//!
//! See the [`App`] type for more details

// Imports
use crate::{Args, ImageLoader, Panel, PanelState, PanelsRenderer, PathLoader, Wgpu};
use anyhow::Context;
use crossbeam::thread;
use parking_lot::Mutex;
use std::{
	sync::atomic::{self, AtomicBool},
	time::Duration,
};
use winit::{
	dpi::{PhysicalPosition, PhysicalSize},
	event::{Event, WindowEvent},
	event_loop::{ControlFlow as EventLoopControlFlow, EventLoop},
	platform::{
		run_return::EventLoopExtRunReturn,
		unix::{WindowBuilderExtUnix, WindowExtUnix, XWindowType},
	},
	window::{Window, WindowBuilder},
};
use x11::xlib;

/// Application state
///
/// Stores all of the application state
pub struct App {
	/// Event loop
	event_loop: EventLoop<!>,

	/// Window
	///
	/// [`Wgpu`] required a static reference to it, which is
	/// why we leak it.
	window: &'static Window,

	/// Wgpu
	wgpu: Wgpu,

	/// Path loader
	_path_loader: PathLoader,

	/// Image loader
	image_loader: ImageLoader,

	/// Panels
	panels: Mutex<Vec<Panel>>,

	/// Panels renderer
	panels_renderer: PanelsRenderer,
}

impl App {
	/// Creates a new app
	#[allow(clippy::future_not_send)] // Unfortunately we can't do much about it, we must build the window in the main thread
	pub async fn new(args: Args) -> Result<Self, anyhow::Error> {
		// Build the window
		let (event_loop, window) = self::create_window(&args)?;

		// Create the wgpu interface
		let wgpu = Wgpu::new(window).await.context("Unable to create renderer")?;

		// Create the path loader
		let path_loader = PathLoader::new(args.images_dir).context("Unable to create path loader")?;

		// Create the image loader
		let image_loader = ImageLoader::new(&path_loader).context("Unable to create image loader")?;

		// Create all panels
		let panels = args
			.panel_geometries
			.iter()
			.map(|&geometry| {
				Panel::new(
					geometry,
					PanelState::Empty,
					args.image_duration,
					args.fade_point,
					args.image_backlog.unwrap_or(1).max(1),
				)
			})
			.collect::<Vec<_>>();
		let panels = Mutex::new(panels);

		// Create the panels renderer
		let panels_renderer = PanelsRenderer::new(wgpu.device(), wgpu.texture_format())
			.await
			.context("Unable to create panels renderer")?;


		Ok(Self {
			event_loop,
			window,
			wgpu,
			_path_loader: path_loader,
			image_loader,
			panels,
			panels_renderer,
		})
	}

	/// Runs the app 'till completion
	pub fn run(mut self) -> Result<(), anyhow::Error> {
		// Start the renderer thread
		let should_quit = AtomicBool::new(false);
		thread::scope(|s| {
			// Spawn the updater thread
			s.builder()
				.name("Updater thread".to_owned())
				.spawn(Self::updater_thread(
					&should_quit,
					&self.wgpu,
					&self.panels,
					&self.panels_renderer,
					&self.image_loader,
				))
				.context("Unable to start renderer thread")?;

			// Spawn the renderer thread
			s.builder()
				.name("Renderer thread".to_owned())
				.spawn(Self::renderer_thread(
					&should_quit,
					&self.wgpu,
					&self.panels,
					self.window,
					&self.panels_renderer,
				))
				.context("Unable to start renderer thread")?;

			// Run event loop in this thread until we quit
			self.event_loop.run_return(|event, _, control_flow| {
				// Set control for to wait for next event, since we're not doing
				// anything else on the main thread
				*control_flow = EventLoopControlFlow::Wait;

				// Then handle the event
				match event {
					Event::WindowEvent { event, .. } => match event {
						WindowEvent::Resized(size) => self.wgpu.resize(size),
						WindowEvent::CloseRequested | WindowEvent::Destroyed => {
							log::warn!("Received close request, closing window");
							*control_flow = EventLoopControlFlow::Exit;
						},
						_ => (),
					},
					_ => (),
				}
			});

			// Notify other threads to quit
			should_quit.store(true, atomic::Ordering::Relaxed);

			Ok(())
		})
		.map_err(|err| anyhow::anyhow!("Unable to start all threads and run event loop: {:?}", err))?
	}

	/// Returns the function to run in the updater thread
	fn updater_thread<'a>(
		should_quit: &'a AtomicBool, wgpu: &'a Wgpu, panels: &'a Mutex<Vec<Panel>>,
		panels_renderer: &'a PanelsRenderer, image_loader: &'a ImageLoader,
	) -> impl FnOnce(&thread::Scope) + 'a {
		move |_| {
			// Duration we're sleep
			let sleep_duration = Duration::from_secs_f32(1.0 / 60.0);

			while !should_quit.load(atomic::Ordering::Relaxed) {
				// Render
				let (res, frame_duration) =
					crate::util::measure(|| Self::update(wgpu, &mut *panels.lock(), panels_renderer, image_loader));
				match res {
					Ok(()) => log::debug!("Took {frame_duration:?} to render"),
					Err(err) => log::warn!("Unable to render: {err:?}"),
				};

				// Then sleep until next frame
				if let Some(duration) = sleep_duration.checked_sub(frame_duration) {
					std::thread::sleep(duration);
				}
			}
		}
	}

	/// Updates
	fn update(
		wgpu: &Wgpu, panels: &mut [Panel], panels_renderer: &PanelsRenderer, image_loader: &ImageLoader,
	) -> Result<(), anyhow::Error> {
		for panel in panels {
			if let Err(err) = panel.update(
				wgpu.device(),
				wgpu.queue(),
				panels_renderer.uniforms_bind_group_layout(),
				panels_renderer.texture_bind_group_layout(),
				image_loader,
			) {
				log::warn!("Unable to update panel: {err:?}");
			}
		}

		Ok(())
	}

	/// Returns the function to run in the renderer thread
	fn renderer_thread<'a>(
		should_quit: &'a AtomicBool, wgpu: &'a Wgpu, panels: &'a Mutex<Vec<Panel>>, window: &'a Window,
		panels_renderer: &'a PanelsRenderer,
	) -> impl FnOnce(&thread::Scope) + 'a {
		move |_| {
			// Duration we're sleep
			let sleep_duration = Duration::from_secs_f32(1.0 / 60.0);

			while !should_quit.load(atomic::Ordering::Relaxed) {
				// Render
				let (res, frame_duration) =
					crate::util::measure(|| Self::render(wgpu, &mut **panels.lock(), window, panels_renderer));
				match res {
					Ok(()) => log::debug!("Took {frame_duration:?} to render"),
					Err(err) => log::warn!("Unable to render: {err:?}"),
				};

				// Then sleep until next frame
				if let Some(duration) = sleep_duration.checked_sub(frame_duration) {
					std::thread::sleep(duration);
				}
			}
		}
	}

	/// Renders
	fn render(
		wgpu: &Wgpu, panels: &mut [Panel], window: &Window, panels_renderer: &PanelsRenderer,
	) -> Result<(), anyhow::Error> {
		wgpu.render(|encoder, view| panels_renderer.render(panels, encoder, view, wgpu.queue(), window.inner_size()))
	}
}

/// Creates the window, as well as the associated event loop
fn create_window(args: &Args) -> Result<(EventLoop<!>, &'static Window), anyhow::Error> {
	// Build the window
	// TODO: Not leak the window
	let event_loop = EventLoop::with_user_event();
	let window = WindowBuilder::new()
		.with_position(PhysicalPosition {
			x: args.window_geometry.pos[0],
			y: args.window_geometry.pos[1],
		})
		.with_inner_size(PhysicalSize {
			width:  args.window_geometry.size[0],
			height: args.window_geometry.size[1],
		})
		.with_x11_window_type(vec![XWindowType::Desktop])
		.build(&event_loop)
		.context("Unable to build window")?;
	let window = Box::leak(Box::new(window));

	// Set the window as always below
	// Note: Required so it doesn't hide itself if the desktop is clicked on
	// SAFETY: TODO
	unsafe {
		self::set_display_always_below(window);
	}
	Ok((event_loop, window))
}

/// Sets the display as always below
///
/// # Safety
/// TODO
#[allow(clippy::expect_used)] // TODO: Refactor all of this
unsafe fn set_display_always_below(window: &Window) {
	// Get the xlib display and window
	let display = window.xlib_display().expect("No `X` display found").cast();
	let window = window.xlib_window().expect("No `X` window found");

	// Flush the existing `XMapRaised`
	unsafe { xlib::XFlush(display) };
	std::thread::sleep(Duration::from_millis(100));

	// Unmap the window temporarily
	unsafe { xlib::XUnmapWindow(display, window) };
	unsafe { xlib::XFlush(display) };
	std::thread::sleep(Duration::from_millis(100));

	// Add the always below hint to the window manager
	{
		let property = unsafe { xlib::XInternAtom(display, b"_NET_WM_STATE\0".as_ptr().cast(), 0) };
		let value = unsafe { xlib::XInternAtom(display, b"_NET_WM_STATE_BELOW\0".as_ptr().cast(), 0) };
		let res = unsafe {
			xlib::XChangeProperty(
				display,
				window,
				property,
				xlib::XA_ATOM,
				32,
				xlib::PropModeAppend,
				(&value as *const u64).cast(),
				1,
			)
		};
		assert_eq!(res, 1, "Unable to change window property");
	}

	// Then remap it
	unsafe { xlib::XMapRaised(display, window) };
	unsafe { xlib::XFlush(display) };
}
