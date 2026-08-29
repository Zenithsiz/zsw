//! Zenithsiz's scrolling wallpaper

#![feature(
	never_type,
	must_not_suspend,
	proc_macro_hygiene,
	stmt_expr_attributes,
	bool_toggle,
	sync_nonpoison,
	nonpoison_mutex,
	thread_sleep_until,
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
mod window;

use {
	self::{args::Args, config::Config, dirs::Dirs, panel::PanelsRendererShared, profile::Profile, renderer::Renderer},
	app_error::Context,
	clap::Parser,
	directories::ProjectDirs,
	std::{
		collections::BTreeMap,
		fs,
		process::ExitCode,
		sync::{Arc, mpsc},
	},
	winit::{
		application::ApplicationHandler,
		event::WindowEvent,
		event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy, OwnedDisplayHandle},
		platform::x11::EventLoopBuilderExtX11,
		window::WindowId,
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
	let config_path = args.config.unwrap_or_else(|| dirs.data_dir().join("config.toml"));
	let config = Config::get_or_create_default(&config_path);
	let dirs = Dirs::new(
		config_path
			.parent()
			.expect("Config file had no parent directory")
			.to_path_buf(),
	);
	let dirs = Arc::new(dirs);
	tracing::debug!("Loaded config: {config:?}");

	logger.set_file(args.log_file.as_deref().or(config.log_file.as_deref()));

	// Create the event loop
	// TODO: Not force x11 once we can get wayland to lower our window on startup
	let event_loop = EventLoop::with_user_event()
		.with_x11()
		.build()
		.context("Unable to build winit event loop")?;

	// Initialize the app
	let mut app = WinitApp::new(
		config,
		dirs,
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
	config:            Config,
	renderer_event_tx: mpsc::Sender<renderer::Event>,
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
		_ = self
			.renderer_event_tx
			.send(renderer::Event::WindowEvent { window_id, event });
	}
}

impl WinitApp {
	/// Creates a new app
	pub async fn new(
		config: Config,
		dirs: Arc<Dirs>,
		display: OwnedDisplayHandle,
		event_loop_proxy: EventLoopProxy<AppEvent>,
	) -> Result<Self, AppError> {
		let wgpu = Wgpu::new(display).await.context("Unable to initialize wgpu")?;
		let panels_renderer_shared = PanelsRendererShared::new(&wgpu);

		let playlists = zsw_util::read_dir_all_toml(dirs.playlists()).context("Unable to create playlists")?;
		let profiles = zsw_util::read_dir_all_toml::<_, Arc<Profile>, BTreeMap<_, _>>(dirs.profiles())
			.context("Unable to create profiles")?;

		// TODO: Make the renderer create the menu and maybe the renderer rx/tx too?
		let (renderer_event_tx, renderer_event_rx) = mpsc::channel();
		let mut renderer = Renderer::new(
			&config,
			event_loop_proxy,
			wgpu,
			panels_renderer_shared,
			playlists,
			profiles,
			renderer_event_rx,
		)
		.context("Unable to build renderer")?;
		zsw_util::spawn_task("Renderer", move || renderer.render());

		Ok(Self {
			config,
			renderer_event_tx,
		})
	}

	/// Initializes the window related things
	pub fn init_window(&self, event_loop: &ActiveEventLoop) -> Result<(), AppError> {
		let windows = window::create(
			event_loop,
			self.config.transparent_windows,
			self.config.monitors.as_deref(),
		)
		.context("Unable to create winit event loop and window")?;
		for app_window in windows {
			_ = self.renderer_event_tx.send(renderer::Event::WindowAdd { app_window });
		}

		Ok(())
	}

	/// Destroys the window related things
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
