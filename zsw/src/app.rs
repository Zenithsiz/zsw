//! App

// Lints
// We need to share a lot of state and we can't couple it together in most cases
#![allow(clippy::too_many_arguments)]

// Modules
mod event_handler;
mod renderer;
mod settings_window;

// Imports
use {
	self::{event_handler::EventHandler, renderer::Renderer, settings_window::SettingsWindow},
	crate::{img, paths, util, Args, Egui, Panel, PanelState, Panels, PanelsProfile, PanelsRenderer, Wgpu},
	anyhow::Context,
	crossbeam::atomic::AtomicCell,
	std::{
		num::NonZeroUsize,
		sync::atomic::{self, AtomicBool},
		thread,
		time::Duration,
	},
	winit::{
		dpi::{PhysicalPosition, PhysicalSize},
		event_loop::EventLoop,
		platform::{
			run_return::EventLoopExtRunReturn,
			unix::{WindowBuilderExtUnix, WindowExtUnix, XWindowType},
		},
		window::{Window, WindowBuilder},
	},
	x11::xlib,
};

/// Runs the application
pub fn run(args: &Args) -> Result<(), anyhow::Error> {
	// Build the window
	let (mut event_loop, window) = self::create_window(args)?;

	// Create the wgpu interface
	let wgpu = Wgpu::new(&window).context("Unable to create renderer")?;

	// Create the paths channel
	let (paths_distributer, paths_rx) = paths::new(args.images_dir.clone());

	// Create the image loader
	let (image_loader, image_receiver) = img::loader::new(paths_rx);

	// Create all panels
	let panels = args
		.panel_geometries
		.iter()
		.map(|&geometry| Panel::new(geometry, PanelState::Empty, args.image_duration, args.fade_point));
	let panels = Panels::new(panels);

	// Create the panels renderer
	let panels_renderer = PanelsRenderer::new(wgpu.device(), wgpu.surface_texture_format())
		.context("Unable to create panels renderer")?;

	// Create egui
	let egui = Egui::new(&window, &wgpu).context("Unable to create egui state")?;

	// Read all profiles
	let _profiles: Vec<PanelsProfile> = match util::parse_json_from_file("zsw_profiles.json") {
		Ok(profiles) => {
			log::info!("Loaded profiles {profiles:#?}");
			profiles
		},
		Err(err) => {
			log::info!("Unable to load profiles: {err:?}");
			vec![]
		},
	};

	let queued_settings_window_open_click = AtomicCell::new(None);
	let should_stop = AtomicBool::new(false);
	let (paint_jobs_tx, paint_jobs_rx) = crossbeam::channel::bounded(0);

	// Create the event handler
	let mut event_handler = EventHandler::new();

	// Create the renderer
	let renderer = Renderer::new(image_receiver);

	// Create the settings window
	let settings_window = SettingsWindow::new(wgpu.surface_size());

	// Start all threads and then wait in the main thread for events
	// Note: The outer result of `scope` can't be `Err` due to a panic in
	//       another thread, since we manually join all threads at the end.
	crossbeam::thread::scope(|s| {
		// Create the thread spawner
		let mut thread_spawner = util::ThreadSpawner::new(s);

		// Spawn the path distributer thread
		thread_spawner.spawn_scoped("Path distributer", || paths_distributer.run())?;

		// Spawn all image loaders
		let loader_threads = thread::available_parallelism().map_or(1, NonZeroUsize::get);
		let loader_fns = vec![image_loader; loader_threads]
			.into_iter()
			.map(|image_loader| move || image_loader.run());
		thread_spawner.spawn_scoped_multiple("Image loader", loader_fns)?;

		// Spawn the settings window thread
		thread_spawner.spawn_scoped("Settings window", || {
			settings_window.run(
				&wgpu,
				&egui,
				&window,
				&panels,
				&paths_distributer,
				&queued_settings_window_open_click,
				&paint_jobs_tx,
			);
			Ok(())
		})?;

		// Spawn the renderer thread
		thread_spawner.spawn_scoped("Renderer", || {
			renderer.run(
				&window,
				&wgpu,
				&panels_renderer,
				&panels,
				&egui,
				&should_stop,
				&paint_jobs_rx,
			);
			Ok(())
		})?;

		// Run event loop in this thread until we quit
		event_loop.run_return(|event, _, control_flow| {
			event_handler.handle_event(&wgpu, &egui, &queued_settings_window_open_click, event, control_flow);
		});

		// Note: In release builds, once we get here, we can just exit,
		//       no need to make the user wait for shutdown code.
		// TODO: Check if anything needs to run drop code, such as possibly
		//       saving all profiles or something similar?
		#[cfg(not(debug_assertions))]
		std::process::exit(0);

		let (res, duration) = util::measure(|| {
			// Stop the renderer
			// Note: Stopping the renderer will cause it to drop the image receiver,
			//       which will stop the image loaders and in turn the path loader.
			should_stop.store(true, atomic::Ordering::Relaxed);

			// Join all thread
			thread_spawner.join_all().context("Unable to join all threads")
		});
		log::info!("Took {duration:?} to join all threads");

		res
	})
	.map_err(|err| anyhow::anyhow!("Unable to start all threads: {err:?}"))?
}

/// Creates the window, as well as the associated event loop
fn create_window(args: &Args) -> Result<(EventLoop<!>, Window), anyhow::Error> {
	// Build the window
	let event_loop = EventLoop::with_user_event();
	log::debug!("Creating window (geometry: {:?})", args.window_geometry);
	let window = WindowBuilder::new()
		.with_position(PhysicalPosition {
			x: args.window_geometry.pos[0],
			y: args.window_geometry.pos[1],
		})
		.with_inner_size(PhysicalSize {
			width:  args.window_geometry.size[0],
			height: args.window_geometry.size[1],
		})
		.with_decorations(false)
		.with_x11_window_type(vec![XWindowType::Desktop])
		.build(&event_loop)
		.context("Unable to build window")?;

	// Set the window as always below
	// Note: Required so it doesn't hide itself if the desktop is clicked on
	// SAFETY: TODO
	unsafe {
		self::set_display_always_below(&window);
	}

	Ok((event_loop, window))
}

/// Sets the display as always below
///
/// # Safety
/// TODO
unsafe fn set_display_always_below(window: &Window) {
	// Get the xlib display and window
	let display = window.xlib_display().expect("No `X` display found").cast();
	let window = window.xlib_window().expect("No `X` window found");

	// Flush the existing `XMapRaised`
	assert_eq!(unsafe { xlib::XFlush(display) }, 1);
	thread::sleep(Duration::from_millis(100));

	// Unmap the window temporarily
	assert_eq!(unsafe { xlib::XUnmapWindow(display, window) }, 1);
	assert_eq!(unsafe { xlib::XFlush(display) }, 1);
	thread::sleep(Duration::from_millis(100));

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
				std::ptr::addr_of!(value).cast(),
				1,
			)
		};
		assert_eq!(res, 1, "Unable to change window property");
	}

	// Then remap it
	assert_eq!(unsafe { xlib::XMapRaised(display, window) }, 1);
	assert_eq!(unsafe { xlib::XFlush(display) }, 1);
}
