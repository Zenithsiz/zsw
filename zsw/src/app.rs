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
	crate::Args,
	anyhow::Context,
	cgmath::{Point2, Vector2},
	std::{iter, num::NonZeroUsize, thread, time::Duration},
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
	zsw_egui::Egui,
	zsw_img::ImageLoader,
	zsw_panels::Panels,
	zsw_playlist::Playlist,
	zsw_profiles::Profiles,
	zsw_util::{FutureRunner, MightLock, Rect, WithSideEffect},
	zsw_wgpu::Wgpu,
};

/// Runs the application
pub fn run(args: &Args) -> Result<(), anyhow::Error> {
	// Build the window
	let (mut event_loop, window) = self::create_window()?;

	// Create the wgpu interface
	let wgpu = Wgpu::new(&window).context("Unable to create renderer")?;

	// Create the playlist
	let playlist = Playlist::new();

	// Create the image loader
	let image_loader = ImageLoader::new();

	// Create the panels
	let panels = Panels::new(wgpu.device(), wgpu.surface_texture_format()).context("Unable to create panels")?;

	// Create egui
	let egui = Egui::new(&window, &wgpu).context("Unable to create egui state")?;

	// Create the profiles
	let profiles = Profiles::new().context("Unable to load profiles")?;

	// Create the event handler
	let mut event_handler = EventHandler::new();

	// Create the renderer
	let renderer = Renderer::new();

	// Create the settings window
	let settings_window = SettingsWindow::new();

	// All runners
	// Note: They must exit outside of the thread scope because
	//       their `run` can last until the very end of the function
	let profile_loader_runner = FutureRunner::new();
	let playlist_runner = FutureRunner::new();
	let image_loader_threads = thread::available_parallelism().map_or(1, NonZeroUsize::get);
	let image_loader_runners = iter::repeat_with(FutureRunner::new)
		.take(image_loader_threads)
		.collect::<Vec<_>>();
	let settings_window_runner = FutureRunner::new();
	let renderer_runner = FutureRunner::new();


	// Start all threads and then wait in the main thread for events
	// Note: The outer result of `scope` can't be `Err` due to a panic in
	//       another thread, since we manually join all threads at the end.
	// DEADLOCK: We ensure all threads lock each lock in the same order,
	//           and that we don't lock them.
	//           All threads ensure they will eventually release any lock
	//           they obtain.
	thread::scope(|s| {
		// Create the thread spawner
		let mut thread_spawner = zsw_util::ThreadSpawner::new(s);

		// Spawn the profile loader if we have any
		if let Some(path) = &args.profile {
			// Note: We don't care whether we got cancelled or returned successfully
			thread_spawner.spawn("Profile loader", || {
				profile_loader_runner
					.run(async {
						match profiles.load(path.clone()) {
							Ok(profile) => {
								log::info!("Successfully loaded profile: {profile:?}");
								profile.apply(&playlist, &panels).await;
							},
							Err(err) => log::warn!("Unable to load profile: {err:?}"),
						}
					})
					.into_ok_or_err();
			})?;
		}

		// Spawn the playlist thread
		thread_spawner.spawn("Playlist", || {
			playlist_runner.run(playlist.run()).into_err();
		})?;

		// Spawn all image loaders
		for (thread_idx, runner) in image_loader_runners.iter().enumerate() {
			thread_spawner.spawn(format!("Image Loader${thread_idx}"), || {
				runner.run(image_loader.run(&playlist)).into_err();
			})?;
		}

		// Spawn the settings window thread
		// DEADLOCK: See above
		thread_spawner.spawn("Settings window", || {
			settings_window_runner
				.run(settings_window.run(&wgpu, &egui, &window, &panels, &playlist, &profiles))
				.map::<!, _>(WithSideEffect::allow::<MightLock<zsw_wgpu::SurfaceLock>>)
				.into_err();
		})?;

		// Spawn the renderer thread
		// DEADLOCK: See above
		thread_spawner.spawn("Renderer", || {
			renderer_runner
				.run(renderer.run(&window, &wgpu, &panels, &egui, &image_loader, &settings_window))
				.map::<!, _>(WithSideEffect::allow::<MightLock<zsw_wgpu::SurfaceLock>>)
				.into_err();
		})?;

		// Run event loop in this thread until we quit
		// DEADLOCK: `run_return` exits once the user requests it.
		event_loop.run_return(|event, _, control_flow| {
			event_handler.handle_event(&wgpu, &egui, &settings_window, event, control_flow);
		});

		// Note: In release builds, once we get here, we can just exit,
		//       no need to make the user wait for shutdown code.
		// TODO: Check if anything needs to run drop code, such as possibly
		//       saving all profiles or something similar?
		#[cfg(not(debug_assertions))]
		std::process::exit(0);

		// Stop all runners at the end
		// Note: Order doesn't matter, as they don't block
		playlist_runner.stop();
		image_loader_runners.iter().for_each(FutureRunner::stop);
		settings_window_runner.stop();
		renderer_runner.stop();

		// Then join all threads
		thread_spawner.join_all().context("Unable to join all threads")?;

		Ok(())
	})
}

/// Creates the window, as well as the associated event loop
fn create_window() -> Result<(EventLoop<!>, Window), anyhow::Error> {
	// Build the window
	let event_loop = EventLoop::with_user_event();

	// Find the window geometry
	// Note: We just merge all monitors' geometry.
	let window_geometry = event_loop
		.available_monitors()
		.map(|monitor| self::monitor_geometry(&monitor))
		.reduce(Rect::merge)
		.context("No monitors found")?;

	log::debug!("Creating window (geometry: {:?})", window_geometry);
	let window = WindowBuilder::new()
		.with_position(PhysicalPosition {
			x: window_geometry.pos[0],
			y: window_geometry.pos[1],
		})
		.with_inner_size(PhysicalSize {
			width:  window_geometry.size[0],
			height: window_geometry.size[1],
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

/// Returns a monitor's geometry
fn monitor_geometry(monitor: &winit::monitor::MonitorHandle) -> Rect<i32, u32> {
	let monitor_pos = monitor.position();
	let monitor_size = monitor.size();
	Rect {
		pos:  Point2::new(monitor_pos.x, monitor_pos.y),
		size: Vector2::new(monitor_size.width, monitor_size.height),
	}
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
