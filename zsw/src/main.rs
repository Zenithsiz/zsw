//! Zenithsiz's scrolling wallpaper

// TODO: When using mailbox (uncapped FPS), a deadlock occurs.
//       Strangely doesn't occur on immediate, might be a driver issue.

// Features
#![feature(
	never_type,
	decl_macro,
	assert_matches,
	type_alias_impl_trait,
	path_file_prefix,
	let_chains,
	exit_status_error,
	closure_track_caller,
	generic_const_exprs,
	must_not_suspend,
	anonymous_lifetime_in_impl_trait,
	try_blocks,
	yeet_expr
)]
#![expect(incomplete_features)]

// Modules
mod args;
mod config;
mod config_dirs;
mod image_loader;
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
		panel::{Panel, PanelShader, PanelsManager, PanelsRenderer},
		playlist::{PlaylistName, Playlists},
		settings_menu::SettingsMenu,
		shared::Shared,
	},
	args::Args,
	cgmath::Point2,
	clap::Parser,
	crossbeam::atomic::AtomicCell,
	directories::ProjectDirs,
	futures::{stream::FuturesUnordered, Future, StreamExt},
	std::{fs, sync::Arc},
	tokio::sync::{mpsc, Mutex, RwLock},
	winit::{
		dpi::{PhysicalPosition, PhysicalSize},
		event::WindowEvent,
		event_loop::EventLoop,
		window::WindowId,
	},
	zsw_egui::{EguiPainter, EguiRenderer},
	zsw_util::{meetup, TokioTaskBlockOn},
	zsw_wgpu::WgpuRenderer,
	zutil_app_error::{app_error, AppError, Context},
};


fn main() -> Result<(), AppError> {
	// Get arguments
	let args = Args::parse();
	init::logger::pre_init::debug(format!("args: {args:?}"));

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
	init::logger::pre_init::debug(format!("config_path: {config_path:?}, config: {config:?}"));

	// Initialize the logger properly now
	init::logger::init(args.log_file.as_deref().or(config.log_file.as_deref()));

	// Initialize and create everything
	init::rayon_pool::init(config.rayon_worker_threads).context("Unable to initialize rayon")?;
	let tokio_runtime =
		init::tokio_runtime::create(config.tokio_worker_threads).context("Unable to create tokio runtime")?;

	// Enter the tokio runtime
	let _runtime_enter = tokio_runtime.enter();

	// Create the event loop
	let event_loop = EventLoop::builder()
		.build()
		.context("Unable to build winit event loop")?;

	// Finally run the app on the event loop
	let (event_tx, event_rx) = mpsc::unbounded_channel();
	event_loop
		.run_app(&mut WinitApp {
			dirs,
			config,
			config_dirs,
			event_rx: Some(event_rx),
			event_tx,
		})
		.context("Unable to run event loop")?;

	Ok(())
}

struct WinitApp {
	dirs:        ProjectDirs,
	config:      Arc<Config>,
	config_dirs: Arc<ConfigDirs>,
	event_rx:    Option<mpsc::UnboundedReceiver<(WindowId, WindowEvent)>>,
	event_tx:    mpsc::UnboundedSender<(WindowId, WindowEvent)>,
}

impl winit::application::ApplicationHandler for WinitApp {
	fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
		// Try to initialize
		let Err(err) = self::run(
			&self.dirs,
			&self.config,
			&self.config_dirs,
			event_loop,
			self.event_rx.take().expect("Already resumed"),
		)
		.block_on() else {
			return;
		};

		// If we get an error, print it and quit
		tracing::error!(?err, "Unable to initialize");
		std::process::exit(1);
	}

	fn window_event(
		&mut self,
		_event_loop: &winit::event_loop::ActiveEventLoop,
		window_id: WindowId,
		event: WindowEvent,
	) {
		let _ = self.event_tx.send((window_id, event));
	}
}

async fn run(
	dirs: &ProjectDirs,
	config: &Arc<Config>,
	config_dirs: &Arc<ConfigDirs>,
	event_loop: &winit::event_loop::ActiveEventLoop,
	mut event_rx: mpsc::UnboundedReceiver<(WindowId, WindowEvent)>,
) -> Result<(), AppError> {
	// TODO: Not leak the window?
	let window = window::create(event_loop).context("Unable to create winit event loop and window")?;
	let window = Box::leak(Box::new(window));
	let (wgpu_shared, wgpu_renderer) = zsw_wgpu::create(window)
		.await
		.context("Unable to create wgpu renderer")?;

	// If the shaders path doesn't exist, write it
	// TODO: Use a virtual filesystem instead?
	let shaders_path = config_dirs.shaders();
	if !fs::exists(shaders_path).context("Unable to check if shaders path exists")? {
		return Err(app_error!("Shaders directory doesn't exist: {shaders_path:?}"));
	}

	let (panels_renderer, panels_renderer_layout) = PanelsRenderer::new(config_dirs, &wgpu_renderer, &wgpu_shared)
		.await
		.context("Unable to create panels renderer")?;
	let (egui_renderer, egui_painter, egui_event_handler) = zsw_egui::create(window, &wgpu_renderer, &wgpu_shared);
	let settings_menu = SettingsMenu::new();

	let playlists = Playlists::load(config_dirs.playlists().to_path_buf())
		.await
		.context("Unable to load playlists")?;

	let panels_manager = PanelsManager::new();

	let upscale_cache_dir = config
		.upscale_cache_dir
		.clone()
		.unwrap_or_else(|| dirs.cache_dir().join("upscale_cache/"));
	let (image_loader, image_requester) = image_loader::create(
		upscale_cache_dir,
		config.upscale_cmd.clone(),
		config.upscale_exclude.clone(),
	)
	.await
	.context("Unable to create image loader")?;

	// Shared state
	let shared = Shared {
		window,
		wgpu: wgpu_shared,
		panels_renderer_layout,
		last_resize: AtomicCell::new(None),
		// TODO: Not have a default of (0,0)?
		cursor_pos: AtomicCell::new(PhysicalPosition::new(0.0, 0.0)),
		config_dirs: Arc::clone(config_dirs),
		panels_manager,
		image_requester,
		cur_panels: Mutex::new(vec![]),
		panels_shader: RwLock::new(PanelShader::None),
		playlists: RwLock::new(playlists),
	};
	let shared = Arc::new(shared);

	let (egui_painter_output_tx, egui_painter_output_rx) = meetup::channel();
	let (panels_updater_output_tx, panels_updater_output_rx) = meetup::channel();


	self::spawn_task("Load default panels", {
		let shared = Arc::clone(&shared);
		let config = Arc::clone(config);
		let config_dirs = Arc::clone(config_dirs);
		|| async move { self::load_default_panels(&config, &config_dirs, shared).await }
	});

	self::spawn_task("Renderer", {
		let shared = Arc::clone(&shared);
		|| {
			self::renderer(
				shared,
				wgpu_renderer,
				panels_renderer,
				egui_renderer,
				egui_painter_output_rx,
				panels_updater_output_rx,
			)
		}
	});

	self::spawn_task("Panels updater", {
		let shared = Arc::clone(&shared);
		|| self::panels_updater(shared, panels_updater_output_tx)
	});

	self::spawn_task("Image loader", || image_loader.run());

	self::spawn_task("Egui painter", {
		let shared = Arc::clone(&shared);
		|| self::egui_painter(shared, egui_painter, settings_menu, egui_painter_output_tx)
	});

	self::spawn_task("Event receiver", {
		let shared = Arc::clone(&shared);
		|| async move {
			while let Some((_, event)) = event_rx.recv().await {
				match event {
					winit::event::WindowEvent::Resized(size) => shared.last_resize.store(Some(Resize { size })),
					winit::event::WindowEvent::CursorMoved { position, .. } => shared.cursor_pos.store(position),
					_ => (),
				}

				egui_event_handler.handle_event(&event).await;
			}

			Ok(())
		}
	});

	Ok(())
}

/// Loads the default panels
async fn load_default_panels(config: &Config, config_dirs: &ConfigDirs, shared: Arc<Shared>) -> Result<(), AppError> {
	// Load the panels
	let shared = &shared;
	let loaded_panels = config
		.default
		.panels
		.iter()
		.map(|default_panel| async move {
			let default_panel_path = config_dirs.panels().join(&default_panel.panel);
			let playlist_name = PlaylistName::from(default_panel.playlist.clone());
			shared
				.panels_manager
				.load(&default_panel_path, playlist_name, shared)
				.await
				.inspect(|panel| tracing::debug!(?panel, "Loaded default panel"))
				.inspect_err(|err| tracing::warn!("Unable to load default panel {default_panel_path:?}: {err:?}"))
				.ok()
		})
		.collect::<FuturesUnordered<_>>()
		.filter_map(async move |opt| opt)
		.collect::<Vec<Panel>>()
		.await;

	// Add the default panels to the current panels
	{
		let mut cur_panels = shared.cur_panels.lock().await;
		cur_panels.extend(loaded_panels);
	}

	// Finally at the end set the shader
	// TODO: Have this come from the config?
	*shared.panels_shader.write().await = PanelShader::FadeOut { strength: 1.5 };

	Ok(())
}

/// Spawns a task
#[track_caller]
pub fn spawn_task<Fut, F, T>(name: impl Into<String>, f: F)
where
	F: FnOnce() -> Fut + Send + 'static,
	Fut: Future<Output = Result<T, AppError>> + Send + 'static,
{
	let name = name.into();

	let _ = tokio::task::Builder::new().name(&name.clone()).spawn(async move {
		let fut = f();

		let id = tokio::task::id();
		tracing::debug!(?name, ?id, "Spawning task");
		match fut.await {
			Ok(_) => tracing::debug!(?name, "Task finished"),
			Err(err) => tracing::warn!(?name, ?err, "Task returned error"),
		}
	});
}

/// Renderer task
async fn renderer(
	shared: Arc<Shared>,
	mut wgpu_renderer: WgpuRenderer,
	mut panels_renderer: PanelsRenderer,
	mut egui_renderer: EguiRenderer,
	egui_painter_output_rx: meetup::Receiver<(Vec<egui::ClippedPrimitive>, egui::TexturesDelta)>,
	panels_updater_output_rx: meetup::Receiver<()>,
) -> Result<!, AppError> {
	let mut egui_paint_jobs = vec![];
	let mut egui_textures_delta = None;
	loop {
		// Meetup with the panels updater
		// Note: We just *try* to meetup, if it hasn't finished yet,
		//       we just re-render the existing panels.
		_ = panels_updater_output_rx.try_recv();

		// Update egui, if available
		if let Some((paint_jobs, textures_delta)) = egui_painter_output_rx.try_recv() {
			egui_paint_jobs = paint_jobs;
			egui_textures_delta = Some(textures_delta);
		}

		// Start rendering
		let mut frame = wgpu_renderer
			.start_render(&shared.wgpu)
			.context("Unable to start frame")?;
		// Render the panels
		{
			let cur_panels = shared.cur_panels.lock().await;

			let shader = *shared.panels_shader.read().await;
			panels_renderer
				.render(
					&mut frame,
					&shared.config_dirs,
					&wgpu_renderer,
					&shared.wgpu,
					&shared.panels_renderer_layout,
					&*cur_panels,
					shader,
				)
				.await
				.context("Unable to render panels")?;
		}

		// Render egui
		egui_renderer
			.render_egui(
				&mut frame,
				shared.window,
				&shared.wgpu,
				&egui_paint_jobs,
				egui_textures_delta.take(),
			)
			.context("Unable to render egui")?;

		// Finish the frame
		frame.finish(&shared.wgpu);

		// Resize if we need to
		if let Some(resize) = shared.last_resize.swap(None) {
			wgpu_renderer
				.resize(&shared.wgpu, resize.size)
				.context("Unable to resize wgpu")?;
			panels_renderer.resize(&wgpu_renderer, &shared.wgpu, resize.size);
		}
	}
}

/// Panel updater task
#[expect(clippy::infinite_loop, reason = "We need this type signature for `spawn_task`")]
async fn panels_updater(shared: Arc<Shared>, panels_updater_output_tx: meetup::Sender<()>) -> Result<!, AppError> {
	loop {
		{
			let mut cur_panels = shared.cur_panels.lock().await;

			for panel in &mut *cur_panels {
				panel
					.update(&shared.wgpu, &shared.panels_renderer_layout, &shared.image_requester)
					.await;
			}
		}

		panels_updater_output_tx.send(()).await;
	}
}

/// Egui painter task
async fn egui_painter(
	shared: Arc<Shared>,
	egui_painter: EguiPainter,
	mut settings_menu: SettingsMenu,
	egui_painter_output_tx: meetup::Sender<(Vec<egui::ClippedPrimitive>, egui::TexturesDelta)>,
) -> Result<!, AppError> {
	loop {
		let full_output_fut = egui_painter.draw(shared.window, async |ctx| {
			// Draw the settings menu
			tokio::task::block_in_place(|| settings_menu.draw(ctx, &shared));

			// Pause any double-clicked panels
			if !ctx.is_pointer_over_area() &&
				ctx.input(|input| input.pointer.button_double_clicked(egui::PointerButton::Primary))
			{
				let cursor_pos = shared.cursor_pos.load();
				let cursor_pos = Point2::new(cursor_pos.x as i32, cursor_pos.y as i32);
				let mut cur_panels = shared.cur_panels.lock().await;
				for panel in &mut *cur_panels {
					for geometry in &panel.geometries {
						if geometry.geometry.contains(cursor_pos) {
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
				let mut cur_panels = shared.cur_panels.lock().await;
				for panel in &mut *cur_panels {
					if !panel
						.geometries
						.iter()
						.any(|geometry| geometry.geometry.contains(cursor_pos))
					{
						continue;
					}

					panel
						.skip(&shared.wgpu, &shared.panels_renderer_layout, &shared.image_requester)
						.await;
				}
			}

			// Scroll panels
			// TODO: Deduplicate this with the above and settings menu.
			if !ctx.is_pointer_over_area() && ctx.input(|input| input.smooth_scroll_delta.y != 0.0) {
				let delta = ctx.input(|input| input.smooth_scroll_delta.y);
				let cursor_pos = shared.cursor_pos.load();
				let cursor_pos = Point2::new(cursor_pos.x as i32, cursor_pos.y as i32);
				let mut cur_panels = shared.cur_panels.lock().await;
				for panel in &mut *cur_panels {
					if !panel
						.geometries
						.iter()
						.any(|geometry| geometry.geometry.contains(cursor_pos))
					{
						continue;
					}

					// TODO: Make this "speed" configurable
					let speed = (panel.state.duration as f32) / 1000.0;
					let frames = (-delta * speed) as i64;
					panel
						.step(
							&shared.wgpu,
							&shared.panels_renderer_layout,
							&shared.image_requester,
							frames,
						)
						.await;
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
