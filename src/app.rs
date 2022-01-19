//! App
//!
//! See the [`App`] type for more details

// Imports
use crate::{Args, Egui, ImageLoader, Panel, PanelState, PanelsRenderer, PathLoader, Wgpu};
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

/// Inner state
// Note: This is required because the event loop can't be
//       shared in between threads, but everything else we need
//       to share.
struct Inner {
	/// Window
	///
	/// [`Wgpu`] required a static reference to it, which is
	/// why we leak it.
	window: &'static Window,

	/// Wgpu
	wgpu: Wgpu,

	/// Path loader
	// Note: Although we have it behind a mutex, we don't need to access it,
	//       it's only there so we can share `self` between threads
	_path_loader: Mutex<PathLoader>,

	/// Image loader
	image_loader: ImageLoader,

	/// Panels
	panels: Mutex<Vec<Panel>>,

	/// Panels renderer
	panels_renderer: PanelsRenderer,

	/// Egui
	egui: Egui,
}

/// Application state
///
/// Stores all of the application state
pub struct App {
	/// Event loop
	event_loop: EventLoop<!>,

	/// Inner
	inner: Inner,
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

		// Create egui
		let egui = Egui::new(window, &wgpu).context("Unable to create egui state")?;

		Ok(Self {
			event_loop,
			inner: Inner {
				window,
				wgpu,
				_path_loader: Mutex::new(path_loader),
				image_loader,
				panels,
				panels_renderer,
				egui,
			},
		})
	}

	/// Runs the app 'till completion
	pub fn run(mut self) -> Result<(), anyhow::Error> {
		// Start all threads and then wait in the main thread for events
		let inner = self.inner;
		let should_quit = AtomicBool::new(false);
		thread::scope(|s| {
			// Spawn the updater thread
			s.builder()
				.name("Updater thread".to_owned())
				.spawn(Self::updater_thread(&inner, &should_quit))
				.context("Unable to start renderer thread")?;

			// Spawn the renderer thread
			s.builder()
				.name("Renderer thread".to_owned())
				.spawn(Self::renderer_thread(&inner, &should_quit))
				.context("Unable to start renderer thread")?;

			// Run event loop in this thread until we quit
			self.event_loop.run_return(|event, _, control_flow| {
				// Update egui
				inner.egui.platform().lock().handle_event(&event);

				// Set control for to wait for next event, since we're not doing
				// anything else on the main thread
				*control_flow = EventLoopControlFlow::Wait;

				// Then handle the event
				match event {
					Event::WindowEvent { event, .. } => match event {
						WindowEvent::Resized(size) => inner.wgpu.resize(size),
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
	fn updater_thread<'a>(inner: &'a Inner, should_quit: &'a AtomicBool) -> impl FnOnce(&thread::Scope) + 'a {
		move |_| {
			// Duration we're sleep
			let sleep_duration = Duration::from_secs_f32(1.0 / 60.0);

			while !should_quit.load(atomic::Ordering::Relaxed) {
				// Render
				let (res, frame_duration) = crate::util::measure(|| Self::update(inner));
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
	fn update(inner: &Inner) -> Result<(), anyhow::Error> {
		let mut panels = inner.panels.lock();
		for panel in &mut *panels {
			if let Err(err) = panel.update(
				inner.wgpu.device(),
				inner.wgpu.queue(),
				inner.panels_renderer.uniforms_bind_group_layout(),
				inner.panels_renderer.texture_bind_group_layout(),
				&inner.image_loader,
			) {
				log::warn!("Unable to update panel: {err:?}");
			}
		}

		Ok(())
	}

	/// Returns the function to run in the renderer thread
	fn renderer_thread<'a>(inner: &'a Inner, should_quit: &'a AtomicBool) -> impl FnOnce(&thread::Scope) + 'a {
		move |_| {
			// Duration we're sleep
			let sleep_duration = Duration::from_secs_f32(1.0 / 60.0);

			// Display the demo application that ships with egui.
			let mut demo_app = egui_demo_lib::WrapApp::default();

			while !should_quit.load(atomic::Ordering::Relaxed) {
				// Render
				let (res, frame_duration) = crate::util::measure(|| Self::render(inner, &mut demo_app));
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
	fn render(inner: &Inner, demo_app: &mut egui_demo_lib::WrapApp) -> Result<(), anyhow::Error> {
		// Draw egui
		// TODO: When this is moved to it's own thread, regardless of issues with
		//       synchronizing the platform, we should synchronize the drawing to ensure
		//       we don't draw twice without displaying, as the first draw would never be
		//       visible to the user.
		let paint_jobs = inner
			.egui
			.draw(inner.window, |ctx, frame| {
				epi::App::update(demo_app, ctx, frame);
				Ok(())
			})
			.context("Unable to draw egui")?;

		inner.wgpu.render(|encoder, surface_view| {
			// Render the panels
			let mut panels = inner.panels.lock();
			inner
				.panels_renderer
				.render(
					&mut *panels,
					encoder,
					surface_view,
					inner.wgpu.queue(),
					inner.window.inner_size(),
				)
				.context("Unable to render panels")?;

			// Render egui
			let window_size = inner.window.inner_size();
			#[allow(clippy::cast_possible_truncation)] // Unfortunately `egui` takes an `f32`
			let screen_descriptor = egui_wgpu_backend::ScreenDescriptor {
				physical_width:  window_size.width,
				physical_height: window_size.height,
				scale_factor:    inner.window.scale_factor() as f32,
			};
			let device = inner.wgpu.device();
			let queue = inner.wgpu.queue();
			let mut egui_render_pass = inner.egui.render_pass().lock();

			// TODO: Check if it's fine to get the platform here without synchronizing
			//       with the drawing.
			let egui_platform = inner.egui.platform().lock();
			egui_render_pass.update_texture(device, queue, &egui_platform.context().font_image());
			egui_render_pass.update_user_textures(device, queue);
			egui_render_pass.update_buffers(device, queue, &paint_jobs, &screen_descriptor);

			// Record all render passes.
			egui_render_pass
				.execute(encoder, surface_view, &paint_jobs, &screen_descriptor, None)
				.context("Unable to render egui")
		})
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
