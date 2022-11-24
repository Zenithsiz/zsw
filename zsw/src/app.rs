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
		image_provider::ImageProvider,
		profile_applier::ProfileApplier,
		resources::{Resources, ResourcesInner},
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
	zsw_egui::{EguiEventHandler, EguiPainter, EguiRenderer},
	zsw_img::ImageLoader,
	zsw_input::{InputReceiver, InputUpdater},
	zsw_panels::{PanelsRenderer, PanelsResource},
	zsw_playlist::{PlaylistReceiver, PlaylistRunner},
	zsw_profiles::Profile,
	zsw_renderer::Renderer,
	zsw_settings_window::{ProfileApplier as _, SettingsWindow},
	zsw_util::{Rect, ResourcesBundle},
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
		playlist_runner,
		playlist_receiver,
		image_loader,
		egui_painter,
		egui_renderer,
		mut egui_event_handler,
		panels_renderer,
		input_receiver,
		mut input_updater,
		settings_window,
	) = self::create_services_resources(Arc::clone(&window)).await?;
	let services = Arc::new(services);
	tracing::debug!(?services, ?resources, "Created services and resources");

	// Create the event handler
	let mut event_handler = EventHandler::new();

	// Try to load the default profile
	// TODO: Not assume a default exists?
	let default_profile = match services
		.profiles_manager
		.load(args.profile.as_ref().cloned().unwrap_or_else(|| "profile.json".into()))
	{
		Ok(profile) => profile,
		Err(err) => return Err(err).context("Unable to load default profile"),
	};

	// Spawn all futures
	let join_handle = self::spawn_services(
		&services,
		&resources,
		playlist_runner,
		playlist_receiver,
		image_loader,
		default_profile,
		egui_painter,
		egui_renderer,
		panels_renderer,
		input_receiver,
		settings_window,
	)
	.context("Unable to spawn all tasks")?;

	// Run the event loop until exit
	let _ = event_loop.run_return(|event, _, control_flow| {
		event_handler
			.handle_event(
				&*services,
				event,
				control_flow,
				&mut egui_event_handler,
				&mut input_updater,
			)
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
		PlaylistRunner,
		PlaylistReceiver,
		ImageLoader,
		EguiPainter,
		EguiRenderer,
		EguiEventHandler,
		PanelsRenderer,
		InputReceiver,
		InputUpdater,
		SettingsWindow,
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
	let (panels_renderer, panels_editor, panels_resource) =
		zsw_panels::create(wgpu.device(), wgpu.surface_texture_format());

	// Create egui
	let (egui_renderer, egui_painter, egui_event_handler) = zsw_egui::create(&window, &wgpu);

	// Create the profiles
	let profiles_manager = zsw_profiles::create();

	// Create the renderer
	let renderer = Renderer::new();

	// Create the settings window
	let settings_window = SettingsWindow::new(&window);

	// Create the input
	let (input_updater, input_receiver) = zsw_input::create();

	// Bundle the services
	let services = Services {
		window,
		wgpu,
		image_receiver,
		playlist_manager,
		profiles_manager,
		panels_editor,
		renderer,
	};

	// Bundle the resources
	let resources = Resources(Arc::new(ResourcesInner {
		panels:       Mutex::new(panels_resource),
		wgpu_surface: Mutex::new(wgpu_surface_resource),
	}));

	Ok((
		services,
		resources,
		playlist_runner,
		playlist_receiver,
		image_loader,
		egui_painter,
		egui_renderer,
		egui_event_handler,
		panels_renderer,
		input_receiver,
		input_updater,
		settings_window,
	))
}

/// Spawns all services and returns a future to join them all
// TODO: Hide future behind a `JoinHandle` type.
#[allow(clippy::needless_pass_by_value)] // Ergonomics
pub fn spawn_services(
	services: &Arc<Services>,
	resources: &Resources,
	playlist_runner: PlaylistRunner,
	playlist_receiver: PlaylistReceiver,
	image_loader: ImageLoader,
	default_profile: Arc<Profile>,
	mut egui_painter: EguiPainter,
	mut egui_renderer: EguiRenderer,
	mut panels_renderer: PanelsRenderer,
	mut input_receiver: InputReceiver,
	mut settings_window: SettingsWindow,
) -> Result<impl Future<Output = Result<(), anyhow::Error>>, anyhow::Error> {
	/// Macro to help spawn a service runner
	macro spawn_service_runner([$($clones:ident),* $(,)?] $name:expr => $runner:expr) {{
		$(
			let $clones = $clones.clone();
		)*
		task::Builder::new().name($name).spawn(async move { $runner.await })
	}}

	// Spawn all
	let profile_applier = ProfileApplier::new();
	let profiles_loader_task = spawn_service_runner!(
		[services, resources, default_profile, profile_applier] "Profiles loader" => async move {
			let mut resources = resources;
			let mut panels_resource = resources.resource::<PanelsResource>().await;
			profile_applier.apply(&default_profile, &services, &mut panels_resource);
		}
	)
	.context("Unable to spawn profile loader task")?;

	let playlist_runner_task = task::Builder::new()
		.name("Playlist runner")
		.spawn_blocking(move || playlist_runner.run())
		.context("Unable to spawn playlist runner task")?;

	// TODO: Use spawn_blocking for these
	// TODO: Dynamically change the number of these to the number of panels / another value
	let image_provider = ImageProvider::new(playlist_receiver);
	let image_loader_tasks = (0..default_profile.panels.len())
		.map(|idx| {
			spawn_service_runner!(
				[image_provider, image_loader] &format!("Image loader #{idx}") => image_loader.run(&image_provider)
			)
		})
		.collect::<Result<Vec<_>, _>>()
		.context("Unable to spawn image loader tasks")?;

	let settings_window_task = spawn_service_runner!(
		[services, resources, profile_applier, input_receiver] "Settings window runner" => settings_window.run(&*services, &mut { resources }, &mut egui_painter, profile_applier, &mut { input_receiver })
	).context("Unable to spawn settings window task")?;
	let renderer_task =
		spawn_service_runner!([services, resources] "Renderer" => services.renderer.run(&*services, &mut { resources }, &mut panels_renderer, &mut egui_renderer, &mut input_receiver))
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
