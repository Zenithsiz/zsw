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
mod shared;
mod window;

// Imports
use {
	self::{
		config::Config,
		dirs::Dirs,
		menu::Menu,
		panel::{Panels, PanelsRenderer, PanelsRendererShared},
		profile::{Profile, ProfileName},
		shared::{Shared, SharedWindow},
	},
	app_error::Context,
	args::Args,
	chrono::TimeDelta,
	clap::Parser,
	core::{clone::Share, time::Duration},
	directories::ProjectDirs,
	euclid::default::Point2D,
	std::{
		collections::{BTreeMap, HashMap},
		fs,
		process::ExitCode,
		sync::{Arc, mpsc},
		thread,
		time::Instant,
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
	zutil_cloned::cloned,
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
	renderer_event_tx: mpsc::Sender<RendererEvent>,
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
				_ = window.renderer_event_tx.send(RendererEvent::WindowEvent { event });
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
			#[cloned(shared = self.shared)]
			zsw_util::spawn_task("Renderer", move || {
				self::renderer(
					&shared,
					shared_window,
					&renderer_event_rx,
					wgpu_renderer,
					panels_renderer,
					egui_renderer,
					&egui_painter,
					&egui_event_handler,
					menu,
				)
			});

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

/// Renderer event
#[derive(Debug)]
enum RendererEvent {
	/// Window event
	WindowEvent { event: WindowEvent },
}

/// Renderer task
fn renderer(
	shared: &Shared,
	mut shared_window: SharedWindow,
	renderer_event_rx: &mpsc::Receiver<RendererEvent>,
	mut wgpu_renderer: WgpuRenderer,
	mut panels_renderer: PanelsRenderer,
	mut egui_renderer: EguiRenderer,
	egui_painter: &EguiPainter,
	egui_event_handler: &EguiEventHandler,
	mut menu: Menu,
) -> Result<(), AppError> {
	let frame_duration = Duration::from_secs_f64(1000.0) / shared_window.monitor_refresh_rate_mhz;
	tracing::info!(
		"Window {:?} refresh rate: {:.2} Hz",
		shared_window.monitor_name,
		f64::from(shared_window.monitor_refresh_rate_mhz) / 1000.0,
	);
	tracing::info!(
		"Window {:?} frame duration: {frame_duration:.2?}",
		shared_window.monitor_name
	);

	let mut next_frame = Instant::now();
	loop {
		let prev_frame_end = Instant::now();
		let cur_frame_start = next_frame;
		next_frame += frame_duration;

		// Wait until the start of the next frame.
		// Note: We do this instead of letting wgpu sleep because all vsync
		//       modes that sleep for us do so with a 3 frame buffer, which
		//       means that the user is always 3 frames behind, and thus can
		//       notice pretty heavy lag (at 60Hz, about 50 ms).
		//       We also set the present mode to mailbox, which means that we
		//       don't get any tearing, but have minimal lag.
		thread::sleep_until(cur_frame_start);

		// If we were too late, we need to skip some frames
		if let Some(late) = prev_frame_end.checked_duration_since(cur_frame_start) &&
			late > frame_duration
		{
			#[expect(clippy::cast_sign_loss, reason = "Durations are always positive")]
			let frames = late.div_duration_f64(frame_duration).floor() as u32;
			tracing::trace!("Frame rendered late {late:.2?}, skipping {frames} frames");
			next_frame += frame_duration * frames;
		}

		// Paint egui
		// TODO: Have `egui_renderer` do this for us on render?
		let (egui_paint_jobs, egui_textures_delta) =
			match self::paint_egui(shared, &mut shared_window, egui_painter, &mut menu) {
				Ok((paint_jobs, textures_delta)) => (paint_jobs, Some(textures_delta)),
				Err(err) => {
					tracing::warn!("Unable to draw egui: {err:?}");
					(vec![], None)
				},
			};

		// Start rendering
		let mut frame = wgpu_renderer
			.start_render(&shared.wgpu)
			.context("Unable to start frame")?;

		// Render panels
		panels_renderer
			.render(
				shared,
				&mut shared_window,
				&mut frame,
				&wgpu_renderer,
				&shared.panels_renderer_shared,
			)
			.context("Unable to render panels")?;

		// Render egui
		egui_renderer
			.render_egui(
				&mut frame,
				&shared_window.window,
				&shared.wgpu,
				&egui_paint_jobs,
				egui_textures_delta,
			)
			.context("Unable to render egui")?;

		// Finish the frame
		if frame.finish(&shared.wgpu) {
			wgpu_renderer
				.reconfigure(&shared.wgpu)
				.context("Unable to reconfigure wgpu")?;
		}

		// Handle events
		let mut resize = None;
		let mut move_pos = None;

		while let Ok(event) = renderer_event_rx.try_recv() {
			tracing::trace!("Received renderer event: {event:?}");
			match event {
				RendererEvent::WindowEvent { event } => {
					if egui_event_handler.handle_event(&event) {
						continue;
					}

					match event {
						WindowEvent::Resized(size) => resize = Some(size),
						WindowEvent::Moved(pos) => move_pos = Some(pos),
						_ => (),
					}
				},
			}
		}

		// Note: When resizing we might receive multiple resize events
		//       per frame, so we only use the latest one from the events,
		//       since resizing twice only has the affect of the last resize.
		if let Some(size) = resize {
			wgpu_renderer
				.resize(&shared.wgpu, size)
				.context("Unable to resize wgpu")?;
			panels_renderer.resize(&wgpu_renderer, &shared.wgpu, size)
		}
		if let Some(pos) = move_pos {
			shared_window.monitor_geometry.pos = euclid::point2(pos.x, pos.y);
		}
	}
}

/// Paints egui
fn paint_egui(
	shared: &Shared,
	shared_window: &mut SharedWindow,
	egui_painter: &EguiPainter,
	menu: &mut Menu,
) -> Result<(Vec<egui::ClippedPrimitive>, egui::TexturesDelta), AppError> {
	let full_output = egui_painter.draw(|ctx| {
		// Draw the menu
		menu.draw(
			ctx,
			&shared.wgpu,
			&shared.playlists,
			&shared.profiles,
			&mut shared_window.panels,
			&shared.event_loop_proxy,
			shared_window.monitor_geometry,
		);


		// Then go through all panels checking for interactions with their geometries
		// TODO: Should this be done here and not somewhere else?
		let Some(pointer_pos) = ctx.input(|input| input.pointer.latest_pos()) else {
			return Ok(());
		};
		let pointer_pos = Point2D::new(pointer_pos.x as i32, pointer_pos.y as i32);
		for panel in shared_window.panels.get_all() {
			// If we're over an egui area, or none of the geometries are underneath the cursor, skip the panel
			if ctx.is_pointer_over_egui() ||
				!panel
					.geometries
					.iter()
					.any(|geometry| geometry.on_window(shared_window.monitor_geometry).contains(pointer_pos))
			{
				continue;
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
					panel::PanelState::Fade(state) => state.skip(&shared.wgpu),
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

						state.step(&shared.wgpu, time_delta);
					},
					panel::PanelState::Slide(_) => (),
				}
			}
		}

		Ok::<_, !>(())
	})?;
	let paint_jobs = egui_painter.tessellate_shapes(full_output.shapes, full_output.pixels_per_point);
	let textures_delta = full_output.textures_delta;

	Ok((paint_jobs, textures_delta))
}

/// App event
#[derive(Clone, Copy, Debug)]
enum AppEvent {
	/// Shutdown
	Shutdown,
}
