//! Services

// Imports
use {
	crate::Args,
	anyhow::Context,
	futures::Future,
	std::{num::NonZeroUsize, sync::Arc, thread},
	tokio::task,
	winit::window::Window,
	zsw_egui::Egui,
	zsw_img::ImageLoader,
	zsw_input::Input,
	zsw_panels::Panels,
	zsw_playlist::Playlist,
	zsw_profiles::Profiles,
	zsw_renderer::Renderer,
	zsw_settings_window::SettingsWindow,
	zsw_util::{ServicesBundle, ServicesContains},
	zsw_wgpu::Wgpu,
};


/// All services
// TODO: Make a macro for service runners to not have to bound everything and then get all the services they need
#[derive(Debug)]
pub struct Services {
	/// Window
	// TODO: Not make an arc
	window: Arc<Window>,

	/// Wgpu
	wgpu: Wgpu,

	/// Playlist
	playlist: Playlist,

	/// Image loader
	image_loader: ImageLoader,

	/// Panels
	panels: Panels,

	/// Egui
	egui: Egui,

	/// Profiles
	profiles: Profiles,

	/// Renderer
	renderer: Renderer,

	/// Settings window
	settings_window: SettingsWindow,

	/// Input
	input: Input,
}

impl Services {
	/// Creates all services
	pub async fn new(window: Arc<Window>) -> Result<Self, anyhow::Error> {
		// Create the wgpu service
		// TODO: Execute future inn background and continue initializing
		let wgpu = Wgpu::new(Arc::clone(&window))
			.await
			.context("Unable to create renderer")?;

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

		// Create the renderer
		let renderer = Renderer::new();

		// Create the settings window
		let settings_window = SettingsWindow::new(&window);

		// Create the input
		let input = Input::new();

		Ok(Self {
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
		})
	}

	/// Spawns all services and returns a future to join them all
	// TODO: Hide future behind a `JoinHandle` type.
	pub fn spawn(self: &Arc<Self>, args: &Args) -> impl Future<Output = Result<(), anyhow::Error>> {
		/// Macro to help spawn a service runner
		macro spawn_service_runner($services:ident => $services_cloned:ident, $name:expr, $runner:expr) {
			task::Builder::new().name($name).spawn({
				let $services_cloned = Arc::clone(&$services);
				async move { $runner.await }
			})
		}

		// Spawn all
		let profiles_loader_task = args.profile.clone().map(move |path| {
			spawn_service_runner!(
				self => services,
				"Profiles loader",
				services.profiles.run_loader_applier(&path, &*services)
			)
		});
		let playlist_task = spawn_service_runner!(self => services, "Playlist runner", services.playlist.run());
		let image_loader_tasks = (0..self::image_loader_tasks())
			.map(
				|idx| spawn_service_runner!(self => services, &format!("Image loader #{idx}"), services.image_loader.run(&*services)),
			)
			.collect::<Vec<_>>();

		let settings_window_task =
			spawn_service_runner!(self => services, "Settings window runner", services.settings_window.run(&*services));
		let renderer_task = spawn_service_runner!(self => services, "Renderer", services.renderer.run(&*services));

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
}

/// Returns the number of tasks to use for the image loader runners
fn image_loader_tasks() -> usize {
	thread::available_parallelism().map_or(1, NonZeroUsize::get)
}

impl ServicesBundle for Services {}

#[duplicate::duplicate_item(
	ty                 field;
	[ Window         ] [ window ];
	[ Wgpu           ] [ wgpu ];
	[ Playlist       ] [ playlist ];
	[ ImageLoader    ] [ image_loader ];
	[ Panels         ] [ panels ];
	[ Egui           ] [ egui ];
	[ Profiles       ] [ profiles ];
	[ Renderer       ] [ renderer ];
	[ SettingsWindow ] [ settings_window ];
	[ Input          ] [ input ];
  )]
impl ServicesContains<ty> for Services {
	fn get(&self) -> &ty {
		&self.field
	}
}
