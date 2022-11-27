//! App

// Lints
// We need to share a lot of state and we can't couple it together in most cases
#![allow(clippy::too_many_arguments)]

// Modules
mod event_handler;
mod image_provider;
mod profile_applier;
mod resources;
mod services;

// Imports
use {
	self::{
		event_handler::EventHandler,
		image_provider::AppRawImageProvider,
		profile_applier::ProfileApplier,
		resources::{Resources, ResourcesInner},
		services::Services,
	},
	crate::Config,
	anyhow::Context,
	cgmath::{Point2, Vector2},
	futures::lock::Mutex,
	std::{num::NonZeroUsize, sync::Arc, thread},
	tokio::task,
	winit::{
		dpi::{PhysicalPosition, PhysicalSize},
		event_loop::{EventLoop, EventLoopBuilder},
		platform::{
			run_return::EventLoopExtRunReturn,
			unix::{WindowBuilderExtUnix, XWindowType},
		},
		window::{Window, WindowBuilder},
	},
	zsw_panels::PanelsResource,
	zsw_renderer::Renderer,
	zsw_settings_window::{ProfileApplier as _, SettingsWindow},
	zsw_util::{Rect, ResourcesBundle},
	zsw_wgpu::Wgpu,
};

/// Runs the application
#[allow(clippy::too_many_lines)] // TODO: Refactor
pub async fn run(config: &Arc<Config>) -> Result<(), anyhow::Error> {
	// Build the window
	let (mut event_loop, window) = self::create_window()?;
	let window = Arc::new(window);

	// Create all services and resources
	// TODO: Execute futures in background and continue initializing
	let (wgpu, mut wgpu_surface_resource) = Wgpu::new(Arc::clone(&window))
		.await
		.context("Unable to create renderer")?;
	let (playlist_runner, playlist_receiver, playlist_manager) = zsw_playlist::create();
	let (image_loader, image_resizer, image_receiver) = zsw_img::loader::create();
	let (mut panels_renderer, panels_editor, panels_resource) = zsw_panels::create(&wgpu, &mut wgpu_surface_resource);
	let (mut egui_renderer, mut egui_painter, mut egui_event_handler) = zsw_egui::create(&window, &wgpu);
	let profiles_manager = zsw_profiles::create();
	let (mut input_updater, mut input_receiver) = zsw_input::create();
	let renderer = Renderer::new();
	let mut settings_window = SettingsWindow::new(&window);
	let mut event_handler = EventHandler::new();
	let profile_applier = ProfileApplier::new();


	// Bundle the services and resources
	let services = Arc::new(Services {
		window,
		wgpu,
		image_receiver,
		playlist_manager,
		profiles_manager,
		panels_editor,
		renderer,
	});
	let resources = Resources(Arc::new(ResourcesInner {
		panels:       Mutex::new(panels_resource),
		wgpu_surface: Mutex::new(wgpu_surface_resource),
	}));
	tracing::debug!(?services, ?resources, "Created services and resources");

	// Spawn all
	let profiles_loader_task = spawn_service_runner!(
		[services, resources, config, profile_applier] "Profiles loader" => async move {
			let mut resources = resources;

			// Load all profiles
			for profile_path in &config.profiles {
				if let Err(err) = services.profiles_manager.load(profile_path.clone()) {
					tracing::warn!("Unable to load profile: {err:?}");
				}
			}

			// Apply the first one
			if let Some((default_profile_path, default_profile)) = services.profiles_manager.first_profile() {
				tracing::info!("Applying default profile {default_profile_path:?}");
				let mut panels_resource = resources.resource::<PanelsResource>().await;
				profile_applier.apply(&default_profile, &services, &mut panels_resource);
			}
		}
	)
	.context("Unable to spawn profile loader task")?;

	let playlist_runner_task = task::Builder::new()
		.name("Playlist runner")
		.spawn_blocking(move || playlist_runner.run())
		.context("Unable to spawn playlist runner task")?;

	let image_provider = AppRawImageProvider::new(playlist_receiver);
	let image_loader_tasks_len = config
		.image_loader_threads
		.or_else(|| thread::available_parallelism().ok())
		.map_or(1, NonZeroUsize::get);
	let image_loader_tasks = (0..image_loader_tasks_len)
		.map(|idx| {
			let image_loader = image_loader.clone();
			let image_provider = image_provider.clone();
			thread::Builder::new()
				.name(format!("ImgLoader${idx}"))
				.spawn(move || image_loader.run(&image_provider))
		})
		.collect::<Result<Vec<_>, _>>()
		.context("Unable to spawn image loader tasks")?;
	let image_resizer_tasks_len = config
		.image_resizer_threads
		.or_else(|| thread::available_parallelism().ok())
		.map_or(1, NonZeroUsize::get);
	let image_resizer_tasks = (0..image_resizer_tasks_len)
		.map(|idx| {
			let image_resizer = image_resizer.clone();
			thread::Builder::new()
				.name(format!("ImgResizer${idx}"))
				.spawn(move || image_resizer.run())
		})
		.collect::<Result<Vec<_>, _>>()
		.context("Unable to spawn image resizer tasks")?;

	let settings_window_task = spawn_service_runner!(
		[services, resources, profile_applier, input_receiver] "Settings window runner" => settings_window.run(&*services, &mut { resources }, &mut egui_painter, profile_applier, &mut { input_receiver })
	).context("Unable to spawn settings window task")?;
	let renderer_task =
		spawn_service_runner!([services, resources] "Renderer" => services.renderer.run(&*services, &mut { resources }, &mut panels_renderer, &mut egui_renderer, &mut input_receiver))
			.context("Unable to spawn renderer task")?;

	// Then create the join future
	let join_handle = async move {
		profiles_loader_task
			.await
			.context("Unable to await for profiles loader runner")?;
		playlist_runner_task
			.await
			.context("Unable to await for playlist runner")?;
		for task in image_loader_tasks {
			task.join()
				.map_err(|err| anyhow::anyhow!("Unable to wait for image loader runner: {err:?}"))?;
		}
		for task in image_resizer_tasks {
			task.join()
				.map_err(|err| anyhow::anyhow!("Unable to wait for image resizer runner: {err:?}"))?;
		}
		settings_window_task
			.await
			.context("Unable to await for settings window runner")?;
		renderer_task.await.context("Unable to await for renderer runner")?;
		Ok::<_, anyhow::Error>(())
	};

	// Run the event loop until exit
	let _ = event_loop.run_return(|event, _, control_flow| {
		event_handler.handle_event(
			&services,
			event,
			control_flow,
			&mut egui_event_handler,
			&mut input_updater,
		);
	});

	// Then join all tasks
	join_handle.await.context("Unable to join all tasks")?;

	Ok(())
}

/// Creates the window, as well as the associated event loop
fn create_window() -> Result<(EventLoop<!>, Window), anyhow::Error> {
	// Build the window
	let event_loop = EventLoopBuilder::with_user_event().build();

	// Find the window geometry
	// Note: We just merge all monitors' geometry.
	let window_geometry = event_loop
		.available_monitors()
		.map(|monitor| self::monitor_geometry(&monitor))
		.reduce(Rect::merge)
		.context("No monitors found")?;

	tracing::debug!(?window_geometry, "Creating window");
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

/// Returns the default number of tasks to use for the image loader runners
fn _default_image_loader_tasks() -> usize {
	thread::available_parallelism().map_or(1, NonZeroUsize::get)
}

/// Macro to help spawn a service runner
macro spawn_service_runner([$($clones:ident),* $(,)?] $name:expr => $runner:expr) {{
	$(
		#[allow(unused_mut)]
		let mut $clones = $clones.clone();
	)*
	task::Builder::new().name($name).spawn(async move { $runner.await })
}}
