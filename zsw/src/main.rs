//! Zenithsiz's scrolling wallpaper

#![feature(
	never_type,
	must_not_suspend,
	proc_macro_hygiene,
	stmt_expr_attributes,
	bool_toggle,
	sync_nonpoison,
	nonpoison_mutex,
	oneshot_channel,
	str_as_str,
	unwrap_infallible,
	share_trait,
	duration_integer_division
)]
#![expect(clippy::too_many_arguments, reason = "TODO: Merge some arguments")]

mod args;
mod config;
mod dirs;
mod menu;
mod panel;
mod playlist;
mod profile;
mod renderer;

use {
	self::{args::Args, config::Config, dirs::Dirs, panel::PanelsRendererShared, profile::Profile, renderer::Renderer},
	app_error::Context,
	clap::Parser,
	core::cell::LazyCell,
	directories::ProjectDirs,
	std::{collections::BTreeMap, fs, process::ExitCode, sync::Arc},
	winit::{
		application::ApplicationHandler,
		event::WindowEvent,
		event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy, OwnedDisplayHandle},
		window::{WindowAttributes, WindowId},
	},
	zsw_util::AppError,
	zsw_wgpu::Wgpu,
	zutil_logger::Logger,
};

fn main() -> ExitCode {
	match self::run() {
		Ok(()) => {
			tracing::info!("Successfully exited");
			ExitCode::SUCCESS
		},
		Err(err) => {
			tracing::error!("Fatal error: {err:?}");
			ExitCode::FAILURE
		},
	}
}

#[tokio::main(flavor = "current_thread")]
async fn run() -> Result<(), AppError> {
	let logger = Logger::builder()
		.filter("wgpu", "warn")
		.filter("naga", "warn")
		.filter("winit", "warn")
		.filter("mio", "warn")
		.build();

	let args = Args::parse();
	tracing::debug!("Args: {args:?}");

	// Create the configuration then load the config
	let dirs = ProjectDirs::from("", "", "zsw").context("Unable to create app directories")?;
	fs::create_dir_all(dirs.data_dir()).context("Unable to create data directory")?;

	let default_config_path = LazyCell::new(|| dirs.data_dir().join("config.toml"));
	let config_path = args.config.as_ref().unwrap_or_else(|| &*default_config_path);
	let config = Config::get_or_create_default(config_path);
	let dirs = Dirs::new(
		config_path
			.parent()
			.expect("Config file had no parent directory")
			.to_path_buf(),
	);
	tracing::debug!("Loaded config: {config:?}");

	logger.set_file(args.log_file.as_deref().or(config.log_file.as_deref()));

	// Create the event loop
	let event_loop = EventLoop::with_user_event()
		.build()
		.context("Unable to build winit event loop")?;

	// Initialize the app
	let mut app = WinitApp::new(
		&args,
		config,
		&dirs,
		event_loop.owned_display_handle(),
		event_loop.create_proxy(),
	)
	.await
	.context("Unable to create winit app")?;

	event_loop.run_app(&mut app).context("Unable to run event loop")?;

	Ok(())
}

#[derive(Debug)]
struct WinitApp {
	renderer: Renderer,
	config:   Config,
}

impl ApplicationHandler<AppEvent> for WinitApp {
	fn resumed(&mut self, event_loop: &ActiveEventLoop) {
		if let Err(err) = self.init_window(event_loop) {
			tracing::warn!("Unable to initialize window: {err:?}");
			event_loop.exit();
		}
	}

	fn suspended(&mut self, event_loop: &ActiveEventLoop) {
		if let Err(err) = self.destroy_window() {
			tracing::warn!("Unable to destroy window: {err:?}");
			event_loop.exit();
		}
	}

	fn user_event(&mut self, event_loop: &ActiveEventLoop, event: AppEvent) {
		match event {
			AppEvent::Shutdown => event_loop.exit(),
		}
	}

	fn window_event(&mut self, _event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
		if let Err(err) = self.renderer.handle_window_event(window_id, &event) {
			tracing::warn!(?window_id, ?event, ?err, "Unable to handle window event");
		}
	}
}

impl WinitApp {
	/// Creates a new app
	pub async fn new(
		args: &Args,
		config: Config,
		dirs: &Dirs,
		display: OwnedDisplayHandle,
		event_loop_proxy: EventLoopProxy<AppEvent>,
	) -> Result<Self, AppError> {
		let wgpu = Wgpu::new(display, args.force_opengl)
			.await
			.context("Unable to initialize wgpu")?;
		let panels_renderer_shared = PanelsRendererShared::new(&wgpu);

		let playlists = zsw_util::read_dir_all_toml(dirs.playlists()).context("Unable to create playlists")?;
		let profiles = zsw_util::read_dir_all_toml::<_, Arc<Profile>, BTreeMap<_, _>>(dirs.profiles())
			.context("Unable to create profiles")?;

		let renderer = Renderer::new(
			&config,
			event_loop_proxy,
			wgpu,
			panels_renderer_shared,
			playlists,
			profiles,
		)
		.context("Unable to build renderer")?;

		Ok(Self { renderer, config })
	}

	/// Initializes the window
	pub fn init_window(&mut self, event_loop: &ActiveEventLoop) -> Result<(), AppError> {
		let window_attrs = WindowAttributes::default()
			.with_title("zsw")
			.with_transparent(self.config.transparent_windows);
		let window = event_loop
			.create_window(window_attrs)
			.context("Unable to create window")?;
		self.renderer
			.set_window(window)
			.context("Unable to set renderer window")?;

		Ok(())
	}

	/// Destroys the window
	#[expect(clippy::needless_pass_by_ref_mut, reason = "We'll use it in the future")]
	pub fn destroy_window(&mut self) -> Result<(), AppError> {
		// TODO: Handle destroying all tasks that use the window
		todo!("Destroying windows isn't supported yet");
	}
}


/// App event
#[derive(Clone, Copy, Debug)]
enum AppEvent {
	/// Shutdown
	Shutdown,
}
