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
	crate::{
		img,
		util::{self, extse::CrossBeamChannelSenderSE, MightBlock},
		Args,
		Egui,
		PanelImageState,
		PanelState,
		Panels,
		PanelsProfile,
		Playlist,
		Wgpu,
	},
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

	// Create the playlist
	let playlist = Playlist::new();

	// Create the image loader
	let (image_loader, image_rx) = img::loader::new();

	// Create all panels
	let panels = args
		.panel_geometries
		.iter()
		.map(|&geometry| PanelState::new(geometry, PanelImageState::Empty, args.image_duration, args.fade_point));
	let panels = Panels::new(panels, image_rx, wgpu.device(), wgpu.surface_texture_format())
		.context("Unable to create panels")?;

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
	let renderer = Renderer::new();

	// Create the settings window
	let settings_window = SettingsWindow::new(wgpu.surface_size());

	// Create the closing signal channels
	let (close_tx, close_rx) = crossbeam::channel::bounded(0);

	// Start all threads and then wait in the main thread for events
	// Note: The outer result of `scope` can't be `Err` due to a panic in
	//       another thread, since we manually join all threads at the end.
	crossbeam::thread::scope(|s| {
		// Create the thread spawner
		let mut thread_spawner = util::ThreadSpawner::new(s);

		// Spawn the playlist loader thread
		thread_spawner.spawn_scoped("Path distributer loader", || {
			playlist.add_dir(&args.images_dir);
			Ok(())
		})?;

		// Spawn the playlist thread
		thread_spawner.spawn_scoped("Path distributer", || {
			playlist.run(&close_rx);
			Ok(())
		})?;

		// Spawn all image loaders
		// DEADLOCK: The path distributer thread ensures it will send paths
		//           The renderer thread ensures it will receive images
		//           We ensure we will keep sending images.
		let loader_threads = thread::available_parallelism().map_or(1, NonZeroUsize::get);
		let loader_fns = vec![image_loader; loader_threads]
			.into_iter()
			.map(|image_loader| || image_loader.run(&playlist).allow::<MightBlock>());
		thread_spawner.spawn_scoped_multiple("Image loader", loader_fns)?;

		// Spawn the settings window thread
		thread_spawner.spawn_scoped("Settings window", || {
			// DEADLOCK: Renderer thread ensures it will receive paint jobs.
			//           We ensure we keep sending paint jobs.
			settings_window
				.run(
					&wgpu,
					&egui,
					&window,
					&panels,
					&playlist,
					&queued_settings_window_open_click,
					&paint_jobs_tx,
				)
				.allow::<MightBlock>();
			Ok(())
		})?;

		// Spawn the renderer thread
		// DEADLOCK: Settings window ensures it will send paint jobs.
		//           We ensure we're not calling it within a [`Wgpu::render`] callback.
		//           This thread ensures it will receive images.
		thread_spawner.spawn_scoped("Renderer", || {
			renderer
				.run(&window, &wgpu, &panels, &egui, &should_stop, &paint_jobs_rx)
				.allow::<MightBlock>();
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
			// DEADLOCK: Stopping the renderer will cause it to drop the image receivers,
			//           which will stop the image loaders, which will in turn stop
			//           the path loader.
			should_stop.store(true, atomic::Ordering::Relaxed);
			if close_tx.send_se(()).allow::<MightBlock>().is_err() {
				log::warn!("Close channel receivers were closed");
			}

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
