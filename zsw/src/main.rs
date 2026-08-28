//! Zenithsiz's scrolling wallpaper

// Features
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
	share_trait
)]
// Lints
#![expect(clippy::too_many_arguments, reason = "TODO: Merge some arguments")]

// Modules
mod args;
mod config;
mod dirs;
mod menu;
mod panel;
mod playlist;
mod profile;
mod renderer;
mod shared;
mod window;

// Imports
use {
	self::{
		args::Args,
		config::Config,
		dirs::Dirs,
		menu::Menu,
		panel::{Panels, PanelsRenderer, PanelsRendererShared},
		profile::{Profile, ProfileName},
		renderer::Renderer,
		shared::{Shared, SharedWindow},
	},
	app_error::Context,
	clap::Parser,
	core::clone::Share,
	directories::ProjectDirs,
	std::{
		collections::{BTreeMap, HashMap},
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
	zsw_egui::{EguiEventHandler, EguiPainter, EguiRenderer},
	zsw_util::AppError,
	zsw_wgpu::{Wgpu, WgpuRenderer},
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
	// Initialize the logger
	let logger = Logger::builder()
		.filter("wgpu", "warn")
		.filter("naga", "warn")
		.filter("winit", "warn")
		.filter("mio", "warn")
		.build();

	// Get arguments
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

	// Set the logger file
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

	// Finally run the app on the event loop
	event_loop.run_app(&mut app).context("Unable to run event loop")?;

	Ok(())
}

#[derive(Debug)]
struct WinitApp {
	windows: HashMap<WindowId, WinitAppWindow>,
	shared:  Arc<Shared>,
}

#[derive(Debug)]
struct WinitAppWindow {
	/// Renderer event sender
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
		match self.windows.get(&window_id) {
			Some(window) => {
				_ = window.renderer_event_tx.send(renderer::Event::WindowEvent { event });
			},
			None => tracing::warn!("Received window event for unknown window {window_id:?}: {event:?}"),
		}
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

		// TODO: Reading of the these should be synchronous, it shouldn't take long to read some
		//       toml files, and it'll simplify other things if we can make it mostly immutable.

		// Create and load the playlists
		let playlists = zsw_util::read_dir_all_toml(dirs.playlists()).context("Unable to create playlists")?;
		let playlists = Arc::new(playlists);

		// Create and load the profiles
		let profiles = zsw_util::read_dir_all_toml::<_, Arc<Profile>, BTreeMap<_, _>>(dirs.profiles())
			.context("Unable to create profiles")?;
		let profiles = Arc::new(profiles);

		// Shared state
		let shared = Shared {
			event_loop_proxy,
			config,
			wgpu,
			panels_renderer_shared,
			playlists,
			profiles,
		};
		let shared = Arc::new(shared);

		Ok(Self {
			windows: HashMap::new(),
			shared,
		})
	}

	/// Initializes the window related things
	pub fn init_window(&mut self, event_loop: &ActiveEventLoop) -> Result<(), AppError> {
		let windows = window::create(
			event_loop,
			self.shared.config.transparent_windows,
			self.shared.config.monitors.as_deref(),
		)
		.context("Unable to create winit event loop and window")?;
		for app_window in windows {
			let window = Arc::new(app_window.window);
			let wgpu_renderer =
				WgpuRenderer::new(window.share(), &self.shared.wgpu).context("Unable to create wgpu renderer")?;

			let msaa_samples = 4;
			let panels_renderer = PanelsRenderer::new(&wgpu_renderer, &self.shared.wgpu, msaa_samples)
				.context("Unable to create panels renderer")?;
			let egui_ctx = egui::Context::default();
			let egui_event_handler = EguiEventHandler::new(&self.shared.wgpu, window.share(), egui_ctx.clone());
			let egui_painter = EguiPainter::new(&egui_event_handler, egui_ctx.clone());
			let egui_renderer = EguiRenderer::new(&wgpu_renderer, &self.shared.wgpu);
			let menu = Menu::new();

			let mut panels = Panels::new();
			if let Some(default_profile_name) = &self.shared.config.default.profile {
				let default_profile_name = default_profile_name.parse::<ProfileName>().into_ok();
				let default_profile = self
					.shared
					.profiles
					.get(&default_profile_name)
					.with_context(|| format!("Unknown profile {:?}", self.shared.config.default.profile))?;
				panels
					.set_profile(default_profile_name, default_profile, &self.shared.playlists)
					.context("Unable to set profile")?;
			}

			let window_id = window.id();
			let shared_window = SharedWindow {
				window,
				monitor_name: app_window.monitor_name,
				monitor_geometry: app_window.monitor_geometry,
				monitor_refresh_rate_mhz: app_window.monitor_refresh_rate_mhz,
				panels,
			};

			let (renderer_event_tx, renderer_event_rx) = mpsc::channel();
			let mut renderer = Renderer::new(
				self.shared.share(),
				shared_window,
				renderer_event_rx,
				wgpu_renderer,
				panels_renderer,
				egui_renderer,
				egui_painter,
				egui_event_handler,
				menu,
			);
			zsw_util::spawn_task("Renderer", move || renderer.run()?);

			_ = self.windows.insert(window_id, WinitAppWindow { renderer_event_tx });
		}

		Ok(())
	}

	/// Destroys the window related things
	#[expect(clippy::needless_pass_by_ref_mut, reason = "We'll use it in the future")]
	pub fn destroy_window(&mut self) -> Result<(), AppError> {
		// TODO: Handle destroying all tasks that use the window
		todo!();
	}
}


/// App event
#[derive(Clone, Copy, Debug)]
enum AppEvent {
	/// Shutdown
	Shutdown,
}
