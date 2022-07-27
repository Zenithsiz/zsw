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
	self::{event_handler::EventHandler, resources::Resources, services::Services},
	crate::Args,
	anyhow::Context,
	cgmath::{Point2, Vector2},
	futures::{lock::Mutex, Future},
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
pub async fn run(args: &Args) -> Result<(), anyhow::Error> {
	// Build the window
	let (mut event_loop, window) = self::create_window()?;
	let window = Arc::new(window);

	// Create all services and resources
	let (services, resources) = self::create_services_resources(Arc::clone(&window)).await?;
	let services = Arc::new(services);
	let resources = Arc::new(resources);
	tracing::debug!(?services, ?resources, "Created services and resources");

	// Create the event handler
	let mut event_handler = EventHandler::new();

	// Spawn all futures
	let join_handle = self::spawn_services(&services, &resources, args);

	// Run the event loop until exit
	event_loop.run_return(|event, _, control_flow| {
		event_handler
			.handle_event(&*services, &*resources, event, control_flow)
			.block_on();
	});

	// Then join all tasks
	join_handle.await.context("Unable to join all tasks")?;

	Ok(())
}

/// Creates all services and resources
pub async fn create_services_resources(window: Arc<Window>) -> Result<(Services, Resources), anyhow::Error> {
	// Create the wgpu service
	// TODO: Execute future in background and continue initializing
	let (wgpu, wgpu_surface_resource) = Wgpu::new(Arc::clone(&window))
		.await
		.context("Unable to create renderer")?;

	// Create the playlist
	let (playlist, playlist_resource) = Playlist::new();

	// Create the image loader
	let image_loader = ImageLoader::new();

	// Create the panels
	let (panels, panels_resource) =
		Panels::new(wgpu.device(), wgpu.surface_texture_format()).context("Unable to create panels")?;

	// Create egui
	let (egui, egui_platform_resource, egui_render_pass_resource, egui_painter_resource) =
		Egui::new(&window, &wgpu).context("Unable to create egui state")?;

	// Create the profiles
	let (profiles, profiles_resource) = Profiles::new();

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
		playlist,
		image_loader,
		panels,
		egui,
		profiles,
		renderer,
		settings_window,
		input,
	};

	// Bundle the resources
	let resources = Resources {
		panels:           Mutex::new(panels_resource),
		playlist:         Mutex::new(playlist_resource),
		profiles:         Mutex::new(profiles_resource),
		wgpu_surface:     Mutex::new(wgpu_surface_resource),
		egui_platform:    Mutex::new(egui_platform_resource),
		egui_render_pass: Mutex::new(egui_render_pass_resource),
		egui_painter:     Mutex::new(egui_painter_resource),
	};

	Ok((services, resources))
}

/// Spawns all services and returns a future to join them all
// TODO: Hide future behind a `JoinHandle` type.
pub fn spawn_services(
	services: &Arc<Services>,
	resources: &Arc<Resources>,
	args: &Args,
) -> impl Future<Output = Result<(), anyhow::Error>> {
	/// Macro to help spawn a service runner
	macro spawn_service_runner([$($clones:ident),* $(,)?] $name:expr => $runner:expr) {
		task::Builder::new().name($name).spawn({
			$(
				let $clones = Arc::clone(&$clones);
			)*
			async move { $runner.await }
		})
	}

	// Spawn all
	let profiles_loader_task = args.profile.clone().map(move |path| {
		spawn_service_runner!(
			[services, resources] "Profiles loader" =>
			services.profiles.run_loader_applier(&path, &*services, &*resources)
		)
	});
	let playlist_task =
		spawn_service_runner!([services, resources] "Playlist runner" => services.playlist.run(&*resources));
	let image_loader_tasks = (0..self::image_loader_tasks())
		.map(|idx| {
			spawn_service_runner!(
				[services, resources] &format!("Image loader #{idx}") => services.image_loader.run(&*services, &*resources)
			)
		})
		.collect::<Vec<_>>();

	let settings_window_task = spawn_service_runner!(
		[services, resources] "Settings window runner" => services.settings_window.run(&*services, &*resources)
	);
	let renderer_task =
		spawn_service_runner!([services, resources] "Renderer" => services.renderer.run(&*services, &*resources));

	// Then create the join future
	async move {
		if let Some(task) = profiles_loader_task {
			task.await.context("Unable to await for profiles loader runner")?;
		}
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

/// Returns the number of tasks to use for the image loader runners
fn image_loader_tasks() -> usize {
	thread::available_parallelism().map_or(1, NonZeroUsize::get)
}
