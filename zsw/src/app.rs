//! App

// Lints
// We need to share a lot of state and we can't couple it together in most cases
#![allow(clippy::too_many_arguments)]

// Modules
mod event_handler;

// Imports
use {
	self::event_handler::EventHandler,
	crate::Args,
	anyhow::Context,
	cgmath::{Point2, Vector2},
	pollster::FutureExt,
	std::{iter, num::NonZeroUsize, thread},
	winit::{
		dpi::{PhysicalPosition, PhysicalSize},
		event_loop::EventLoop,
		platform::{
			run_return::EventLoopExtRunReturn,
			unix::{WindowBuilderExtUnix, XWindowType},
		},
		window::{Window, WindowBuilder},
	},
	zsw_egui::Egui,
	zsw_img::ImageLoader,
	zsw_panels::Panels,
	zsw_playlist::Playlist,
	zsw_profiles::Profiles,
	zsw_renderer::Renderer,
	zsw_settings_window::SettingsWindow,
	zsw_util::{FutureRunner, MightBlock, Rect, WithSideEffect},
	zsw_wgpu::Wgpu,
};

/// Runs the application
#[allow(clippy::too_many_lines)] // TODO: Refactor
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
	// Note: They must exists outside of the thread scope because
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
	// DEADLOCK: We ensure all threads lock each lock in the same order,
	//           and that we don't lock them.
	thread::scope(|s| {
		// Create the thread spawner
		let mut thread_spawner = zsw_util::ThreadSpawner::new(s);

		// Spawn the profile loader if we have any
		// DEADLOCK: See above
		//           [`zsw_profiles::ProfilesLock`]
		//           - [`zsw_playlist::PlaylistLock`]
		//             - [`zsw_panels::PanelsLock`]
		if let Some(path) = &args.profile {
			thread_spawner.spawn("Profile loader", || {
				// Note: We don't care whether we got cancelled or returned successfully
				profile_loader_runner
					.run(profiles.run_loader_applier(path, &playlist, &panels))
					.map(WithSideEffect::allow::<MightBlock>)
					.into_ok_or_err();
			})?;
		}

		// Spawn the playlist thread
		// DEADLOCK: See above
		//           [`zsw_playlist::PlaylistLock`]
		thread_spawner.spawn("Playlist", || {
			playlist_runner
				.run(playlist.run())
				.map::<!, _>(WithSideEffect::allow::<MightBlock>)
				.into_err();
		})?;

		// Spawn all image loaders
		// DEADLOCK: See above
		//           [`zsw_playlist::PlaylistLock`]
		for (thread_idx, runner) in image_loader_runners.iter().enumerate() {
			thread_spawner.spawn(format!("Image Loader${thread_idx}"), || {
				runner
					.run(image_loader.run(&playlist))
					.map::<!, _>(WithSideEffect::allow::<MightBlock>)
					.into_err();
			})?;
		}

		// Spawn the settings window thread
		// DEADLOCK: See above
		//           [`zsw_wgpu::SurfaceLock`]
		//           - [`zsw_egui::PlatformLock`]
		//             - [`zsw_profiles::ProfilesLock`]
		//               - [`zsw_playlist::PlaylistLock`]
		//                 - [`zsw_panels::PanelsLock`]
		thread_spawner.spawn("Settings window", || {
			settings_window_runner
				.run(settings_window.run(&wgpu, &egui, &window, &panels, &playlist, &profiles))
				.map::<!, _>(WithSideEffect::allow::<MightBlock>)
				.into_err();
		})?;

		// Spawn the renderer thread
		// DEADLOCK: See above
		//           [`zsw_panels::PanelsLock`]
		//           [`zsw_wgpu::SurfaceLock`]
		//           - [`zsw_panels::PanelsLock`]
		//           - [`zsw_egui::RenderPassLock`]
		//             - [`zsw_egui::PlatformLock`]
		thread_spawner.spawn("Renderer", || {
			renderer_runner
				.run(renderer.run(&window, &wgpu, &panels, &egui, &image_loader, &settings_window))
				.map::<!, _>(WithSideEffect::allow::<MightBlock>)
				.into_err();
		})?;

		// Run event loop in this thread until we quit
		// DEADLOCK: `run_return` exits once the user requests it.
		//           See above
		//           [`zsw_egui::PlatformLock`]
		// Note: Doesn't make sense to use a runner here, since nothing will call `stop`.
		event_loop.run_return(|event, _, control_flow| {
			event_handler
				.handle_event(&wgpu, &egui, &settings_window, &panels, event, control_flow)
				.block_on()
				.allow::<MightBlock>();
		});

		// Note: In release builds, once we get here, we can just exit,
		//       no need to make the user wait for shutdown code.
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
