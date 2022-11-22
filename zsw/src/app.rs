//! App

// Lints
// We need to share a lot of state and we can't couple it together in most cases
#![allow(clippy::too_many_arguments)]

// Modules
mod event_handler;
mod resources;
mod services;

// Imports
use {
	self::{
		event_handler::EventHandler,
		resources::{Resources, ResourcesMut},
		services::Services,
	},
	crate::Args,
	anyhow::Context,
	cgmath::{Point2, Vector2},
	futures::{lock::Mutex, Future},
	pollster::FutureExt,
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
	zsw_egui::Egui,
	zsw_img::ImageLoader,
	zsw_input::Input,
	zsw_panels::Panels,
	zsw_playlist::{PlaylistManager, PlaylistReceiver, PlaylistRunner},
	zsw_profiles::{Profile, ProfilesManager},
	zsw_renderer::Renderer,
	zsw_settings_window::SettingsWindow,
	zsw_util::Rect,
	zsw_wgpu::Wgpu,
};

/// Runs the application
pub async fn run(args: &Args) -> Result<(), anyhow::Error> {
	// Build the window
	let (mut event_loop, window) = self::create_window()?;
	let window = Arc::new(window);

	// Create all services and resources
	// TODO: Create and spawn all services in the same function
	let (
		services,
		resources,
		resources_mut,
		playlist_runner,
		playlist_receiver,
		playlist_manager,
		profiles_manager,
		image_loader,
	) = self::create_services_resources(Arc::clone(&window)).await?;
	let services = Arc::new(services);
	let resources = Arc::new(resources);
	tracing::debug!(?services, ?resources, "Created services and resources");

	// Create the event handler
	let mut event_handler = EventHandler::new();

	// Try to load the default profile
	// TODO: Not assume a default exists?
	let default_profile =
		match profiles_manager.load(args.profile.as_ref().cloned().unwrap_or_else(|| "profile.json".into())) {
			Ok(profile) => profile,
			Err(err) => return Err(err).context("Unable to load default profile"),
		};

	// Spawn all futures
	let join_handle = self::spawn_services(
		&services,
		&resources,
		resources_mut,
		playlist_runner,
		playlist_receiver,
		playlist_manager,
		profiles_manager,
		image_loader,
		default_profile,
	)
	.context("Unable to spawn all tasks")?;

	// Run the event loop until exit
	let _ = event_loop.run_return(|event, _, control_flow| {
		event_handler
			.handle_event(&*services, &*resources, event, control_flow)
			.block_on();
	});

	// Then join all tasks
	join_handle.await.context("Unable to join all tasks")?;

	Ok(())
}

/// Creates all services and resources
pub async fn create_services_resources(
	window: Arc<Window>,
) -> Result<
	(
		Services,
		Resources,
		ResourcesMut,
		PlaylistRunner,
		PlaylistReceiver,
		PlaylistManager,
		ProfilesManager,
		ImageLoader,
	),
	anyhow::Error,
> {
	// Create the wgpu service
	// TODO: Execute future in background and continue initializing
	let (wgpu, wgpu_surface_resource) = Wgpu::new(Arc::clone(&window))
		.await
		.context("Unable to create renderer")?;

	// Create the playlist
	let (playlist_runner, playlist_receiver, playlist_manager) = zsw_playlist::create();

	// Create the image loader
	let (image_loader, image_receiver) = zsw_img::service::create();

	// Create the panels
	let (panels, panels_resource) =
		Panels::new(wgpu.device(), wgpu.surface_texture_format()).context("Unable to create panels")?;

	// Create egui
	let (egui, egui_platform_resource, egui_render_pass_resource, egui_painter_resource) =
		Egui::new(&window, &wgpu).context("Unable to create egui state")?;

	// Create the profiles
	let profiles_manager = zsw_profiles::create();

	// Create the renderer
	let renderer = Renderer::new();

	// Create the settings window
	let settings_window = SettingsWindow::new(&window);

	// Create the input
	let input = Input::new();

	// Bundle the services
	let services = Services {
		window,
		wgpu,
		image_receiver,
		panels,
		egui,
		renderer,
		settings_window,
		input,
	};

	// Bundle the resources
	let resources = Resources {
		panels:           Mutex::new(panels_resource),
		wgpu_surface:     Mutex::new(wgpu_surface_resource),
		egui_platform:    Mutex::new(egui_platform_resource),
		egui_render_pass: Mutex::new(egui_render_pass_resource),
	};

	let resources_mut = ResourcesMut {
		egui_painter: egui_painter_resource,
	};

	Ok((
		services,
		resources,
		resources_mut,
		playlist_runner,
		playlist_receiver,
		playlist_manager,
		profiles_manager,
		image_loader,
	))
}

/// Spawns all services and returns a future to join them all
// TODO: Hide future behind a `JoinHandle` type.
#[allow(clippy::needless_pass_by_value)] // Ergonomics
pub fn spawn_services(
	services: &Arc<Services>,
	resources: &Arc<Resources>,
	mut resources_mut: ResourcesMut,
	playlist_runner: PlaylistRunner,
	playlist_receiver: PlaylistReceiver,
	playlist_manager: PlaylistManager,
	profiles_manager: ProfilesManager,
	image_loader: ImageLoader,
	default_profile: Arc<Profile>,
) -> Result<impl Future<Output = Result<(), anyhow::Error>>, anyhow::Error> {
	/// Macro to help spawn a service runner
	macro spawn_service_runner([$($clones:ident),* $(,)?] $name:expr => $runner:expr) {{
		$(
			let $clones = $clones.clone();
		)*
		task::Builder::new().name($name).spawn(async move { $runner.await })
	}}

	// Spawn all
	let profiles_loader_task = spawn_service_runner!(
		[services, resources, playlist_manager, default_profile] "Profiles loader" => async move {
			let mut panels_resource = resources.panels.lock().await;
			default_profile.apply(&playlist_manager, &services.panels, &mut panels_resource);
		}
	)
	.context("Unable to spawn profile loader task")?;

	let playlist_runner_task = task::Builder::new()
		.name("Playlist runner")
		.spawn_blocking(move || playlist_runner.run())
		.context("Unable to spawn playlist runner task")?;

	// TODO: Use spawn_blocking for these
	// TODO: Dynamically change the number of these to the number of panels / another value
	let image_loader_tasks = (0..default_profile.panels.len())
		.map(|idx| {
			spawn_service_runner!(
				[playlist_receiver, image_loader] &format!("Image loader #{idx}") => image_loader.run(playlist_receiver)
			)
		})
		.collect::<Result<Vec<_>, _>>()
		.context("Unable to spawn image loader tasks")?;

	let settings_window_task = spawn_service_runner!(
		[services, resources] "Settings window runner" => services.settings_window.run(&*services, &*resources, &mut resources_mut.egui_painter, playlist_manager, profiles_manager)
	).context("Unable to spawn settings window task")?;
	let renderer_task =
		spawn_service_runner!([services, resources] "Renderer" => services.renderer.run(&*services, &*resources))
			.context("Unable to spawn renderer task")?;

	// Then create the join future
	Ok(async move {
		profiles_loader_task
			.await
			.context("Unable to await for profiles loader runner")?;
		playlist_runner_task
			.await
			.context("Unable to await for playlist runner")?;
		for task in image_loader_tasks {
			task.await.context("Unable to wait for image loader runner")?;
		}
		settings_window_task
			.await
			.context("Unable to await for settings window runner")?;
		renderer_task.await.context("Unable to await for renderer runner")?;
		Ok(())
	})
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
