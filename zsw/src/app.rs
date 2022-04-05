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
	futures::future::OptionFuture,
	pollster::FutureExt,
	std::{num::NonZeroUsize, sync::Arc, thread},
	tokio::task,
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
	zsw_input::Input,
	zsw_panels::Panels,
	zsw_playlist::Playlist,
	zsw_profiles::Profiles,
	zsw_renderer::Renderer,
	zsw_settings_window::SettingsWindow,
	zsw_util::Rect,
	zsw_wgpu::Wgpu,
};

/// Runs the application
// TODO: Not arc everything
#[allow(clippy::too_many_lines)] // TODO: Refactor
#[allow(clippy::future_not_send)] // We only want this to run in the main thread anyway, we spawn everything else
pub async fn run(args: Arc<Args>) -> Result<(), anyhow::Error> {
	// Build the window
	let (mut event_loop, window) = self::create_window()?;
	let window = Arc::new(window);

	// Create the wgpu interface
	// TODO: Execute future inn background and continue initializing
	let wgpu = Wgpu::new(Arc::clone(&window))
		.await
		.context("Unable to create renderer")?;
	let wgpu = Arc::new(wgpu);

	// Create the playlist
	let playlist = Playlist::new();
	let playlist = Arc::new(playlist);

	// Create the image loader
	let image_loader = ImageLoader::new();
	let image_loader = Arc::new(image_loader);

	// Create the panels
	let panels = Panels::new(wgpu.device(), wgpu.surface_texture_format()).context("Unable to create panels")?;
	let panels = Arc::new(panels);

	// Create egui
	let egui = Egui::new(&window, &wgpu).context("Unable to create egui state")?;
	let egui = Arc::new(egui);

	// Create the profiles
	let profiles = Profiles::new().context("Unable to load profiles")?;
	let profiles = Arc::new(profiles);

	// Create the event handler
	let mut event_handler = EventHandler::new();

	// Create the renderer
	let renderer = Renderer::new();
	let renderer = Arc::new(renderer);

	// Create the settings window
	let settings_window = SettingsWindow::new();
	let settings_window = Arc::new(settings_window);

	// Create the input
	let input = Input::new();
	let input = Arc::new(input);

	// TODO: Bundle all of these onto a single struct to pass onto the runners,
	//       via some generic

	// Then add all futures
	let profiles_loader_task: OptionFuture<_> = args
		.profile
		.clone()
		.map({
			let profiles = Arc::clone(&profiles);
			let playlist = Arc::clone(&playlist);
			let panels = Arc::clone(&panels);
			move |path| {
				task::Builder::new()
					.name("Profiles loader")
					.spawn(async move { profiles.run_loader_applier(&path, &playlist, &panels).await })
			}
		})
		.into();
	let playlist_task = task::Builder::new().name("Playlist runner").spawn({
		let playlist = Arc::clone(&playlist);
		async move { playlist.run().await }
	});
	let image_loader_tasks = thread::available_parallelism().map_or(1, NonZeroUsize::get);
	let image_loader_tasks = (0..image_loader_tasks)
		.map(|idx| {
			let image_loader = Arc::clone(&image_loader);
			let playlist = Arc::clone(&playlist);
			task::Builder::new()
				.name(&format!("Image loader #{idx}"))
				.spawn(async move { image_loader.run(&playlist).await })
		})
		.collect::<Vec<_>>();

	let settings_window_task = task::Builder::new().name("Settings window runner").spawn({
		let settings_window = Arc::clone(&settings_window);
		let wgpu = Arc::clone(&wgpu);
		let egui = Arc::clone(&egui);
		let window = Arc::clone(&window);
		let profiles = Arc::clone(&profiles);
		let playlist = Arc::clone(&playlist);
		let panels = Arc::clone(&panels);
		let renderer = Arc::clone(&renderer);
		async move {
			settings_window
				.run(&wgpu, &egui, &window, &panels, &playlist, &profiles, &renderer)
				.await;
		}
	});
	let renderer_task = task::Builder::new().name("Renderer runner").spawn({
		let wgpu = Arc::clone(&wgpu);
		let egui = Arc::clone(&egui);
		let window = Arc::clone(&window);
		let panels = Arc::clone(&panels);
		let renderer = Arc::clone(&renderer);
		let input = Arc::clone(&input);
		async move {
			renderer
				.run(&window, &input, &wgpu, &panels, &egui, &image_loader)
				.await;
		}
	});

	// Run the event loop until exit
	event_loop.run_return(|event, _, control_flow| {
		event_handler
			.handle_event(&wgpu, &egui, &settings_window, &input, event, control_flow)
			.block_on();
	});

	// Then join all tasks
	let _ = profiles_loader_task
		.await
		.transpose()
		.context("Unable to await for profiles loader runner")?;
	playlist_task.await.context("Unable to await for playlist runner")?;
	for task in image_loader_tasks {
		task.await.context("Unable to wait for image loader runner")?;
	}
	settings_window_task
		.await
		.context("Unable to await for settings window runner")?;
	renderer_task.await.context("Unable to await for renderer runner")?;

	Ok(())
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
