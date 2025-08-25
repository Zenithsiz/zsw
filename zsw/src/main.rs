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
	stmt_expr_attributes
)]

// Modules
mod args;
mod config;
mod config_dirs;
mod init;
mod panel;
mod playlist;
mod settings_menu;
mod shared;
mod window;

// Imports
use {
	self::{
		config::Config,
		config_dirs::ConfigDirs,
		panel::{PanelImages, PanelName, Panels, PanelsGeometryUniforms, PanelsRenderer, PanelsRendererLayouts},
		playlist::{PlaylistName, PlaylistPlayer, Playlists},
		settings_menu::SettingsMenu,
		shared::{Shared, SharedWindow},
	},
	app_error::{AppError, Context, app_error},
	args::Args,
	cgmath::Point2,
	chrono::TimeDelta,
	clap::Parser,
	crossbeam::atomic::AtomicCell,
	directories::ProjectDirs,
	futures::{Future, StreamExt, stream::FuturesUnordered},
	std::{
		collections::{HashMap, hash_map},
		fs,
		sync::Arc,
	},
	tokio::sync::{Mutex, mpsc},
	winit::{
		dpi::{PhysicalPosition, PhysicalSize},
		event::WindowEvent,
		event_loop::EventLoop,
		platform::{run_on_demand::EventLoopExtRunOnDemand, x11::EventLoopBuilderExtX11},
		window::WindowId,
	},
	zsw_egui::{EguiEventHandler, EguiPainter, EguiRenderer},
	zsw_util::{TokioTaskBlockOn, meetup},
	zsw_wgpu::WgpuRenderer,
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

	// Initialize and create everything
	init::rayon_pool::init(config.rayon_worker_threads).context("Unable to initialize rayon")?;
	let tokio_runtime =
		init::tokio_runtime::create(config.tokio_worker_threads).context("Unable to create tokio runtime")?;

	// Enter the tokio runtime
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
	event_loop
		.run_app_on_demand(&mut app)
		.context("Unable to run event loop")?;

	tracing::info!("Successfully shutting down");
	Ok(())
}

struct WinitApp {
	event_tx:      HashMap<WindowId, mpsc::UnboundedSender<WindowEvent>>,
	shared:        Arc<Shared>,
	shared_window: Vec<Arc<SharedWindow>>,
}

impl winit::application::ApplicationHandler<AppEvent> for WinitApp {
	fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
		if let Err(err) = self.init_window(event_loop) {
			tracing::warn!("Unable to initialize window: {}", err.pretty());
			event_loop.exit();
		}
	}

	fn suspended(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
		if let Err(err) = self.destroy_window() {
			tracing::warn!("Unable to destroy window: {}", err.pretty());
			event_loop.exit();
		}
	}

	fn user_event(&mut self, event_loop: &winit::event_loop::ActiveEventLoop, event: AppEvent) {
		match event {
			AppEvent::Shutdown => event_loop.exit(),
		}
	}

	fn window_event(
		&mut self,
		_event_loop: &winit::event_loop::ActiveEventLoop,
		window_id: WindowId,
		event: WindowEvent,
	) {
		match self.event_tx.get(&window_id) {
			Some(event_tx) => _ = event_tx.send(event),
			None => tracing::warn!("Received window event for unknown window {window_id:?}: {event:?}"),
		}
	}
}

impl WinitApp {
	/// Creates a new app
	pub async fn new(
		config: Arc<Config>,
		config_dirs: Arc<ConfigDirs>,
		event_loop_proxy: winit::event_loop::EventLoopProxy<AppEvent>,
	) -> Result<Self, AppError> {
		// If the shaders path doesn't exist, write it
		// TODO: Use a virtual filesystem instead?
		let shaders_path = config_dirs.shaders();
		if !fs::exists(shaders_path).context("Unable to check if shaders path exists")? {
			return Err(app_error!("Shaders directory doesn't exist: {shaders_path:?}"));
		}

		let wgpu_shared = zsw_wgpu::get_or_create_shared()
			.await
			.context("Unable to initialize wgpu")?;
		let panels_renderer_layouts = PanelsRendererLayouts::new(wgpu_shared);

		let playlists = Playlists::new(config_dirs.playlists().to_path_buf());
		let panels = Panels::new(config_dirs.panels().to_path_buf());

		// Shared state
		let shared = Shared {
			event_loop_proxy,
			last_resize: AtomicCell::new(None),
			// TODO: Not have a default of (0,0)?
			cursor_pos: AtomicCell::new(PhysicalPosition::new(0.0, 0.0)),
			config_dirs: Arc::clone(&config_dirs),
			wgpu: wgpu_shared,
			panels_renderer_layouts,
			panels,
			playlists,
			panels_images: Mutex::new(HashMap::new()),
		};
		let shared = Arc::new(shared);

		#[cloned(shared, config)]
		self::spawn_task("Load default panels", async move {
			self::load_default_panels(&config, &shared).await
		});

		Ok(Self {
			event_tx: HashMap::new(),
			shared,
			shared_window: vec![],
		})
	}

	/// Initializes the window related things
	pub fn init_window(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) -> Result<(), AppError> {
		let windows = window::create(event_loop).context("Unable to create winit event loop and window")?;
		for app_window in windows {
			let window = Arc::new(app_window.window);
			let wgpu_renderer =
				WgpuRenderer::new(Arc::clone(&window), self.shared.wgpu).context("Unable to create wgpu renderer")?;

			let panels_renderer =
				PanelsRenderer::new(&wgpu_renderer, self.shared.wgpu).context("Unable to create panels renderer")?;
			let egui_event_handler = EguiEventHandler::new(&window);
			let egui_painter = EguiPainter::new(&egui_event_handler);
			let egui_renderer = EguiRenderer::new(&wgpu_renderer, self.shared.wgpu);
			let settings_menu = SettingsMenu::new();

			let (egui_painter_output_tx, egui_painter_output_rx) = meetup::channel();

			let shared_window = SharedWindow {
				_monitor_name: app_window.monitor_name,
				monitor_geometry: app_window.monitor_geometry,
				window,
				panels_geometry_uniforms: Mutex::new(PanelsGeometryUniforms::new()),
			};
			let shared_window = Arc::new(shared_window);

			#[cloned(shared = self.shared, shared_window)]
			self::spawn_task("Renderer", async move {
				self::renderer(
					&shared,
					&shared_window,
					wgpu_renderer,
					panels_renderer,
					egui_renderer,
					egui_painter_output_rx,
				)
				.await
			});


			#[cloned(shared = self.shared, shared_window)]
			self::spawn_task("Egui painter", async move {
				self::egui_painter(
					&shared,
					&shared_window,
					egui_painter,
					settings_menu,
					egui_painter_output_tx,
				)
				.await
			});

			let (event_tx, mut event_rx) = mpsc::unbounded_channel();
			_ = self.event_tx.insert(shared_window.window.id(), event_tx);

			#[cloned(shared = self.shared)]
			self::spawn_task("Event receiver", async move {
				while let Some(event) = event_rx.recv().await {
					match event {
						winit::event::WindowEvent::Resized(size) => shared.last_resize.store(Some(Resize { size })),
						winit::event::WindowEvent::CursorMoved { position, .. } => shared.cursor_pos.store(position),
						_ => (),
					}

					egui_event_handler.handle_event(&event).await;
				}

				Ok(())
			});

			self.shared_window.push(shared_window);
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

/// Loads the default panels
async fn load_default_panels(config: &Config, shared: &Arc<Shared>) -> Result<(), AppError> {
	// Load the panels
	config
		.default
		.panels
		.iter()
		.map(async |default_panel| {
			if let Err(err) = self::load_default_panel(default_panel, shared).await {
				tracing::warn!(
					"Unable to load default panel {:?} (playlist: {:?}): {}",
					default_panel.panel,
					default_panel.playlist,
					err.pretty()
				);
			}
		})
		.collect::<FuturesUnordered<_>>()
		.collect::<()>()
		.await;

	Ok(())
}

/// Loads a default panel
async fn load_default_panel(default_panel: &config::ConfigPanel, shared: &Arc<Shared>) -> Result<(), AppError> {
	let panel_name = PanelName::from(default_panel.panel.clone());
	let playlist_name = PlaylistName::from(default_panel.playlist.clone());

	_ = shared
		.panels
		.load(panel_name.clone())
		.await
		.context("Unable to load panel")?;
	tracing::debug!("Loaded default panel {panel_name:?}");

	// Finally spawn a task to load the playlist player
	#[cloned(shared)]
	self::spawn_task(format!("Load playlist for {panel_name:?}"), async move {
		let playlist = shared
			.playlists
			.load(playlist_name.clone())
			.await
			.context("Unable to load playlist")?;
		tracing::debug!("Loaded default playlist {playlist_name:?}");

		let playlist_player = PlaylistPlayer::new(&playlist).await;

		let panel_images = PanelImages::new(playlist_player, shared.wgpu, &shared.panels_renderer_layouts);
		match shared.panels_images.lock().await.entry(panel_name.clone()) {
			hash_map::Entry::Occupied(_) =>
				tracing::warn!("Panel {panel_name:?} changed playlist before playlist {playlist_name:?} could load"),
			hash_map::Entry::Vacant(entry) => _ = entry.insert(panel_images),
		}
		tracing::debug!("Loaded default panel images {panel_name:?}");

		Ok(())
	});

	Ok(())
}

/// Spawns a task
#[track_caller]
pub fn spawn_task<Fut>(name: impl Into<String>, fut: Fut)
where
	Fut: Future<Output = Result<(), AppError>> + Send + 'static,
{
	let name = name.into();

	let _ = tokio::task::Builder::new().name(&name.clone()).spawn(async move {
		let id = tokio::task::id();
		tracing::debug!("Spawning task {name:?} ({id:?})");
		match fut.await {
			Ok(()) => tracing::debug!("Task {name:?} ({id:?}) finished"),
			Err(err) => tracing::warn!("Task {name:?} ({id:?}) returned error: {}", err.pretty()),
		}
	});
}

/// Renderer task
async fn renderer(
	shared: &Shared,
	shared_window: &SharedWindow,
	mut wgpu_renderer: WgpuRenderer,
	mut panels_renderer: PanelsRenderer,
	mut egui_renderer: EguiRenderer,
	egui_painter_output_rx: meetup::Receiver<(Vec<egui::ClippedPrimitive>, egui::TexturesDelta)>,
) -> Result<(), AppError> {
	let mut egui_paint_jobs = vec![];
	let mut egui_textures_delta = None;
	loop {
		// Update egui, if available
		if let Some((paint_jobs, textures_delta)) = egui_painter_output_rx.try_recv() {
			egui_paint_jobs = paint_jobs;
			egui_textures_delta = Some(textures_delta);
		}

		// Start rendering
		let mut frame = wgpu_renderer
			.start_render(shared.wgpu)
			.context("Unable to start frame")?;

		{
			let mut panels_images = shared.panels_images.lock().await;
			let mut panels_geometry_uniforms = shared_window.panels_geometry_uniforms.lock().await;

			panels_renderer
				.render(
					&mut frame,
					&shared.config_dirs,
					&wgpu_renderer,
					shared.wgpu,
					&shared.panels_renderer_layouts,
					&mut panels_geometry_uniforms,
					&shared_window.monitor_geometry,
					&shared.panels,
					&mut panels_images,
				)
				.await
				.context("Unable to render panels")?;
		}

		// Render egui
		egui_renderer
			.render_egui(
				&mut frame,
				&shared_window.window,
				shared.wgpu,
				&egui_paint_jobs,
				egui_textures_delta.take(),
			)
			.context("Unable to render egui")?;

		// Finish the frame
		if frame.finish(shared.wgpu) {
			wgpu_renderer
				.reconfigure(shared.wgpu)
				.context("Unable to reconfigure wgpu")?;
		}

		// Resize if we need to
		if let Some(resize) = shared.last_resize.swap(None) {
			wgpu_renderer
				.resize(shared.wgpu, resize.size)
				.context("Unable to resize wgpu")?;
			panels_renderer.resize(&wgpu_renderer, shared.wgpu, resize.size);
		}
	}
}

/// Egui painter task
async fn egui_painter(
	shared: &Shared,
	shared_window: &SharedWindow,
	egui_painter: EguiPainter,
	mut settings_menu: SettingsMenu,
	egui_painter_output_tx: meetup::Sender<(Vec<egui::ClippedPrimitive>, egui::TexturesDelta)>,
) -> Result<(), AppError> {
	loop {
		let full_output_fut = egui_painter.draw(&shared_window.window, async |ctx| {
			// Draw the settings menu
			tokio::task::block_in_place(|| settings_menu.draw(ctx, shared, shared_window));

			// Pause any double-clicked panels
			if !ctx.is_pointer_over_area() &&
				ctx.input(|input| input.pointer.button_double_clicked(egui::PointerButton::Primary))
			{
				let cursor_pos = shared.cursor_pos.load();
				let cursor_pos = Point2::new(cursor_pos.x as i32, cursor_pos.y as i32);

				for panel in shared.panels.get_all().await {
					let mut panel = panel.lock().await;

					for geometry in &panel.geometries {
						if geometry
							.geometry_on(&shared_window.monitor_geometry)
							.contains(cursor_pos)
						{
							panel.state.paused ^= true;
							break;
						}
					}
				}
			}

			// Skip any ctrl-clicked/middle clicked panels
			// TODO: Deduplicate this with the above and settings menu.
			if !ctx.is_pointer_over_area() &&
				ctx.input(|input| {
					(input.pointer.button_clicked(egui::PointerButton::Primary) && input.modifiers.ctrl) ||
						input.pointer.button_clicked(egui::PointerButton::Middle)
				}) {
				let cursor_pos = shared.cursor_pos.load();
				let cursor_pos = Point2::new(cursor_pos.x as i32, cursor_pos.y as i32);

				let mut panels_images = shared.panels_images.lock().await;

				for panel in shared.panels.get_all().await {
					let mut panel = panel.lock().await;

					if !panel.geometries.iter().any(|geometry| {
						geometry
							.geometry_on(&shared_window.monitor_geometry)
							.contains(cursor_pos)
					}) {
						continue;
					}

					let Some(panel_images) = panels_images.get_mut(&panel.name) else {
						continue;
					};
					panel.skip(panel_images, shared.wgpu, &shared.panels_renderer_layouts);
				}
			}

			// Scroll panels
			// TODO: Deduplicate this with the above and settings menu.
			if !ctx.is_pointer_over_area() && ctx.input(|input| input.smooth_scroll_delta.y != 0.0) {
				let delta = ctx.input(|input| input.smooth_scroll_delta.y);
				let cursor_pos = shared.cursor_pos.load();
				let cursor_pos = Point2::new(cursor_pos.x as i32, cursor_pos.y as i32);

				let mut panels_images = shared.panels_images.lock().await;

				for panel in shared.panels.get_all().await {
					let mut panel = panel.lock().await;

					if !panel.geometries.iter().any(|geometry| {
						geometry
							.geometry_on(&shared_window.monitor_geometry)
							.contains(cursor_pos)
					}) {
						continue;
					}

					// TODO: Make this "speed" configurable
					// TODO: Perform the conversion better without going through nanos
					let speed = 1.0 / 1000.0;
					let time_delta_abs = panel.state.duration.mul_f32(delta.abs() * speed);
					let time_delta_abs =
						TimeDelta::from_std(time_delta_abs).expect("Offset didn't fit into time delta");
					let time_delta = match delta.is_sign_positive() {
						true => -time_delta_abs,
						false => time_delta_abs,
					};

					let Some(panel_images) = panels_images.get_mut(&panel.name) else {
						continue;
					};
					panel.step(panel_images, shared.wgpu, &shared.panels_renderer_layouts, time_delta);
				}
			}

			Ok::<_, !>(())
		});
		let full_output = full_output_fut.await?;
		let paint_jobs = egui_painter
			.tessellate_shapes(full_output.shapes, full_output.pixels_per_point)
			.await;
		let textures_delta = full_output.textures_delta;

		egui_painter_output_tx.send((paint_jobs, textures_delta)).await;
	}
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
