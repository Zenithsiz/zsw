//! Zenithsiz's scrolling wallpaper

// Features
#![feature(
	never_type,
	decl_macro,
	exit_status_error,
	must_not_suspend,
	try_blocks,
	yeet_expr,
	iter_partition_in_place,
	type_alias_impl_trait,
	proc_macro_hygiene,
	stmt_expr_attributes,
	nonpoison_mutex,
	sync_nonpoison,
	duration_millis_float,
	try_trait_v2,
	async_fn_traits,
	unwrap_infallible,
	macro_attr,
	default_field_values
)]
// Lints
#![expect(clippy::too_many_arguments, reason = "TODO: Merge some arguments")]

// Modules
mod args;
mod config;
mod config_dirs;
mod display;
mod init;
mod metrics;
mod panel;
mod playlist;
mod profile;
mod settings_menu;
mod shared;
mod window;

// Imports
use {
	self::{
		config::Config,
		config_dirs::ConfigDirs,
		display::Displays,
		metrics::Metrics,
		panel::{PanelGeometry, Panels, PanelsRenderer, PanelsRendererShared},
		playlist::Playlists,
		profile::{ProfileName, Profiles},
		settings_menu::SettingsMenu,
		shared::Shared,
		window::WindowMonitorNames,
	},
	app_error::Context,
	args::Args,
	cgmath::Point2,
	chrono::TimeDelta,
	clap::Parser,
	crossbeam::atomic::AtomicCell,
	directories::ProjectDirs,
	std::{collections::HashMap, fs, sync::Arc, time::Instant},
	winit::{
		application::ApplicationHandler,
		dpi::PhysicalSize,
		event::WindowEvent,
		event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
		platform::{run_on_demand::EventLoopExtRunOnDemand, x11::EventLoopBuilderExtX11},
		window::{Window, WindowId},
	},
	zsw_egui::{EguiEventHandler, EguiPainter, EguiRenderer},
	zsw_util::{AppError, Rect, TokioTaskBlockOn},
	zsw_wgpu::{Wgpu, WgpuRenderer},
	zutil_cloned::cloned,
};


fn main() -> Result<(), AppError> {
	// Initialize stderr-only logging
	let logger = init::Logger::init_temp();

	// Get arguments
	let args = Args::parse();
	tracing::debug!("Args: {args:?}");

	// Create the configuration then load the config
	let dirs = ProjectDirs::from("", "", "zsw").context("Unable to create app directories")?;
	fs::create_dir_all(dirs.data_dir()).context("Unable to create data directory")?;
	let config_path = args.config.unwrap_or_else(|| dirs.data_dir().join("config.toml"));
	let config = Config::get_or_create_default(&config_path);
	let config = Arc::new(config);
	let config_dirs = ConfigDirs::new(
		config_path
			.parent()
			.expect("Config file had no parent directory")
			.to_path_buf(),
	);
	let config_dirs = Arc::new(config_dirs);
	tracing::debug!("Loaded config: {config:?}");

	// Initialize the logger properly now
	logger.init_global(args.log_file.as_deref().or(config.log_file.as_deref()));

	// Initialize the tokio runtime
	let tokio_runtime =
		init::tokio_runtime::create(config.tokio_worker_threads).context("Unable to create tokio runtime")?;
	let _runtime_enter = tokio_runtime.enter();

	// Create the event loop
	// TODO: Not force x11 once we can get wayland to lower our window on startup
	let mut event_loop = EventLoop::with_user_event()
		.with_x11()
		.build()
		.context("Unable to build winit event loop")?;

	// Initialize the app
	let mut app = WinitApp::new(config, config_dirs, event_loop.create_proxy())
		.block_on()
		.context("Unable to create winit app")?;

	// Finally run the app on the event loop
	tokio::task::block_in_place(|| event_loop.run_app_on_demand(&mut app)).context("Unable to run event loop")?;

	tracing::info!("Successfully shutting down");
	Ok(())
}

struct WinitApp {
	window_event_handlers: HashMap<WindowId, EguiEventHandler>,
	shared:                Arc<Shared>,
}

impl ApplicationHandler<AppEvent> for WinitApp {
	fn resumed(&mut self, event_loop: &ActiveEventLoop) {
		if let Err(err) = self.init_window(event_loop) {
			tracing::warn!("Unable to initialize window: {}", err.pretty());
			event_loop.exit();
		}
	}

	fn suspended(&mut self, event_loop: &ActiveEventLoop) {
		if let Err(err) = self.destroy_window() {
			tracing::warn!("Unable to destroy window: {}", err.pretty());
			event_loop.exit();
		}
	}

	fn user_event(&mut self, event_loop: &ActiveEventLoop, event: AppEvent) {
		match event {
			AppEvent::Shutdown => event_loop.exit(),
		}
	}

	fn window_event(&mut self, _event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
		match event {
			WindowEvent::Resized(size) => self.shared.last_resize.store(Some(Resize { size })),
			WindowEvent::CursorMoved { position, .. } => self.shared.cursor_pos.store(Some(position)),
			_ => (),
		}

		match self.window_event_handlers.get(&window_id) {
			Some(egui_event_handler) => egui_event_handler.handle_event(&event).block_on(),
			None => tracing::warn!("Received window event for unknown window {window_id:?}: {event:?}"),
		}
	}
}

impl WinitApp {
	/// Creates a new app
	pub async fn new(
		config: Arc<Config>,
		config_dirs: Arc<ConfigDirs>,
		event_loop_proxy: EventLoopProxy<AppEvent>,
	) -> Result<Self, AppError> {
		let wgpu = Wgpu::new().await.context("Unable to initialize wgpu")?;
		let panels_renderer_shared = PanelsRendererShared::new(&wgpu);

		// Create and stat loading the displays
		let displays = Displays::new(config_dirs.displays().to_path_buf())
			.await
			.context("Unable to create displays")?;
		let displays = Arc::new(displays);
		#[cloned(displays)]
		zsw_util::spawn_task("Load displays", async move { displays.load_all().await });

		// Create and stat loading the playlists
		let playlists = Playlists::new(config_dirs.playlists().to_path_buf())
			.await
			.context("Unable to create playlists")?;
		let playlists = Arc::new(playlists);
		#[cloned(playlists)]
		zsw_util::spawn_task("Load playlists", async move { playlists.load_all().await });

		// Create and stat loading the profiles
		let profiles = Profiles::new(config_dirs.profiles().to_path_buf())
			.await
			.context("Unable to create profiles")?;
		let profiles = Arc::new(profiles);
		#[cloned(profiles)]
		zsw_util::spawn_task("Load profiles", async move { profiles.load_all().await });

		// Shared state
		let shared = Shared {
			event_loop_proxy,
			last_resize: AtomicCell::new(None),
			cursor_pos: AtomicCell::new(None),
			wgpu,
			panels_renderer_shared,
			displays,
			playlists,
			profiles,
			panels: Arc::new(Panels::new()),
			metrics: Metrics::new(),
			window_monitor_names: WindowMonitorNames::new(),
		};
		let shared = Arc::new(shared);

		if let Some(profile) = &config.default.profile {
			let profile_name = ProfileName::from(profile.clone());

			#[cloned(shared)]
			zsw_util::spawn_task("Load default profile", async move {
				shared
					.panels
					.set_profile(profile_name, &shared.displays, &shared.playlists, &shared.profiles)
					.await
					.context("Unable to set profile")
			});
		}

		Ok(Self {
			window_event_handlers: HashMap::new(),
			shared,
		})
	}

	/// Initializes the window related things
	pub fn init_window(&mut self, event_loop: &ActiveEventLoop) -> Result<(), AppError> {
		let windows = window::create(event_loop).context("Unable to create winit event loop and window")?;
		for app_window in windows {
			self.shared
				.window_monitor_names
				.add(app_window.window.id(), app_window.monitor_name);

			let window = Arc::new(app_window.window);
			let wgpu_renderer =
				WgpuRenderer::new(Arc::clone(&window), &self.shared.wgpu).context("Unable to create wgpu renderer")?;

			let msaa_samples = 4;
			let panels_renderer = PanelsRenderer::new(&wgpu_renderer, &self.shared.wgpu, msaa_samples)
				.context("Unable to create panels renderer")?;
			let egui_event_handler = EguiEventHandler::new(&window);
			let egui_painter = EguiPainter::new(&egui_event_handler);
			let egui_renderer = EguiRenderer::new(&wgpu_renderer, &self.shared.wgpu);
			let settings_menu = SettingsMenu::new();

			#[cloned(shared = self.shared, window)]
			zsw_util::spawn_task("Renderer", async move {
				self::renderer(
					&shared,
					&window,
					wgpu_renderer,
					panels_renderer,
					egui_renderer,
					egui_painter,
					settings_menu,
				)
				.await
			});

			_ = self.window_event_handlers.insert(window.id(), egui_event_handler);
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

/// Renderer task
async fn renderer(
	shared: &Shared,
	window: &Window,
	mut wgpu_renderer: WgpuRenderer,
	mut panels_renderer: PanelsRenderer,
	mut egui_renderer: EguiRenderer,
	egui_painter: EguiPainter,
	mut settings_menu: SettingsMenu,
) -> Result<(), AppError> {
	loop {
		// TODO: Only update this when we receive a move/resize event instead of querying each frame?
		let window_geometry = self::window_geometry(window)?;

		// Paint egui
		// TODO: Have `egui_renderer` do this for us on render?
		#[time(frame_paint_egui)]
		let (egui_paint_jobs, egui_textures_delta) =
			match self::paint_egui(shared, window, &egui_painter, &mut settings_menu, window_geometry).await {
				Ok((paint_jobs, textures_delta)) => (paint_jobs, Some(textures_delta)),
				Err(err) => {
					tracing::warn!("Unable to draw egui: {}", err.pretty());
					(vec![], None)
				},
			};

		// Start rendering
		#[time(frame_render_start)]
		let mut frame = wgpu_renderer
			.start_render(&shared.wgpu)
			.context("Unable to start frame")?;

		// Render panels
		#[time(frame_render_panels)]
		panels_renderer
			.render(
				&mut frame,
				&wgpu_renderer,
				&shared.wgpu,
				&shared.panels_renderer_shared,
				&shared.panels,
				&shared.metrics,
				window,
				window_geometry,
			)
			.await
			.context("Unable to render panels")?;

		// Render egui
		#[time(frame_render_egui)]
		egui_renderer
			.render_egui(&mut frame, window, &shared.wgpu, &egui_paint_jobs, egui_textures_delta)
			.context("Unable to render egui")?;

		// Finish the frame
		#[time(frame_render_finish)]
		let () = if frame.finish(&shared.wgpu) {
			wgpu_renderer
				.reconfigure(&shared.wgpu)
				.context("Unable to reconfigure wgpu")?;
		};

		// Resize if we need to
		#[time(frame_resize)]
		let () = if let Some(resize) = shared.last_resize.swap(None) {
			wgpu_renderer
				.resize(&shared.wgpu, resize.size)
				.context("Unable to resize wgpu")?;
			panels_renderer.resize(&wgpu_renderer, &shared.wgpu, resize.size);
		};


		shared
			.metrics
			.render_frame_times(window.id())
			.await
			.add(metrics::RenderFrameTime {
				paint_egui:    frame_paint_egui,
				render_start:  frame_render_start,
				render_panels: frame_render_panels,
				render_egui:   frame_render_egui,
				render_finish: frame_render_finish,
				resize:        frame_resize,
			});
	}
}

/// Paints egui
async fn paint_egui(
	shared: &Shared,
	window: &Window,
	egui_painter: &EguiPainter,
	settings_menu: &mut SettingsMenu,
	window_geometry: Rect<i32, u32>,
) -> Result<(Vec<egui::ClippedPrimitive>, egui::TexturesDelta), AppError> {
	// Adjust cursor pos to account for the scale factor
	let scale_factor = window.scale_factor();
	let cursor_pos = shared.cursor_pos.load();

	let full_output_fut =
		egui_painter.draw(window, async |ctx| {
			// Draw the settings menu
			let cursor_pos = cursor_pos.map(|cursor_pos| cursor_pos.cast::<f32>().to_logical(scale_factor));
			tokio::task::block_in_place(|| {
				settings_menu.draw(
					ctx,
					&shared.wgpu,
					&shared.displays,
					&shared.playlists,
					&shared.profiles,
					&shared.panels,
					&shared.metrics,
					&shared.window_monitor_names,
					&shared.event_loop_proxy,
					cursor_pos,
					window_geometry,
				)
			});


			// Then go through all panels checking for interactions with their geometries
			let Some(cursor_pos) = shared.cursor_pos.load() else {
				return Ok(());
			};
			let cursor_pos = Point2::new(cursor_pos.x as i32, cursor_pos.y as i32);
			shared
				.panels
				.for_each(async |panel| {
					let panel = &mut *panel.lock().await;
					let display = panel.display.read().await;

					// If we're over an egui area, or none of the geometries are underneath the cursor, skip the panel
					if ctx.is_pointer_over_area() ||
						!display.geometries.iter().any(|&geometry| {
							PanelGeometry::geometry_on(geometry, window_geometry).contains(cursor_pos)
						}) {
						return;
					}

					// Pause any double-clicked panels
					if ctx.input(|input| input.pointer.button_double_clicked(egui::PointerButton::Primary)) {
						#[expect(clippy::match_same_arms, reason = "We'll be changing them soon")]
						match &mut panel.state {
							panel::PanelState::None(_) => (),
							panel::PanelState::Fade(state) => state.toggle_paused(),
							panel::PanelState::Slide(_) => (),
						}
					}

					// Skip any ctrl-clicked/middle clicked panels
					if ctx.input(|input| {
						(input.pointer.button_clicked(egui::PointerButton::Primary) && input.modifiers.ctrl) ||
							input.pointer.button_clicked(egui::PointerButton::Middle)
					}) {
						#[expect(clippy::match_same_arms, reason = "We'll be changing them soon")]
						match &mut panel.state {
							panel::PanelState::None(_) => (),
							panel::PanelState::Fade(state) => state.skip(&shared.wgpu).await,
							panel::PanelState::Slide(_) => (),
						}
					}

					// Scroll panels
					let scroll_delta = ctx.input(|input| input.smooth_scroll_delta.y);
					if scroll_delta != 0.0 {
						#[expect(clippy::match_same_arms, reason = "We'll be changing them soon")]
						match &mut panel.state {
							panel::PanelState::None(_) => (),
							panel::PanelState::Fade(state) => {
								// TODO: Make this "speed" configurable
								// TODO: Perform the conversion better without going through nanos
								let speed = 1.0 / 1000.0;
								let time_delta_abs = state.duration().mul_f32(scroll_delta.abs() * speed);
								let time_delta_abs =
									TimeDelta::from_std(time_delta_abs).expect("Offset didn't fit into time delta");
								let time_delta = match scroll_delta.is_sign_positive() {
									true => -time_delta_abs,
									false => time_delta_abs,
								};

								state.step(&shared.wgpu, time_delta).await;
							},
							panel::PanelState::Slide(_) => (),
						}
					}
				})
				.await;

			Ok::<_, !>(())
		});
	let full_output = full_output_fut.await?;
	let paint_jobs = egui_painter
		.tessellate_shapes(full_output.shapes, full_output.pixels_per_point)
		.await;
	let textures_delta = full_output.textures_delta;

	Ok((paint_jobs, textures_delta))
}

/// Gets the window geometry for a window
fn window_geometry(window: &Window) -> Result<Rect<i32, u32>, AppError> {
	let window_pos = window.inner_position().context("Unable to get window position")?;
	let window_size = window.inner_size();
	Ok(Rect {
		pos:  cgmath::point2(window_pos.x, window_pos.y),
		size: cgmath::vec2(window_size.width, window_size.height),
	})
}

/// A resize
#[derive(Clone, Copy, Debug)]
pub struct Resize {
	/// New size
	size: PhysicalSize<u32>,
}

/// App event
#[derive(Clone, Copy, Debug)]
enum AppEvent {
	/// Shutdown
	Shutdown,
}

// TODO: Not configure this out on ra once it accepts `attr` macros.
// TODO: Allow usage on `if cond { ... }`.
#[cfg(not(rust_analyzer))]
macro time {
	attr($name:ident) ($s:stmt) => {
		let start = Instant::now();
		$s;
		let $name = start.elapsed();
	},

	attr($name:ident) (let $binding:pat = $e:expr;) => {
		let start = Instant::now();
		let $binding = $e;
		let $name = start.elapsed();
	},
}
