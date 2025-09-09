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
	path_add_extension
)]
// Lints
#![expect(clippy::too_many_arguments, reason = "TODO: Merge some arguments")]

// Modules
mod args;
mod config;
mod config_dirs;
mod display;
mod init;
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
		panel::{
			Panel,
			PanelFadeShader,
			PanelFadeState,
			PanelNoneState,
			PanelState,
			PanelsRenderer,
			PanelsRendererShared,
		},
		playlist::{PlaylistItemKind, PlaylistName, PlaylistPlayer, Playlists},
		profile::{ProfileName, ProfilePanelFadeShaderInner, ProfilePanelShader, Profiles},
		settings_menu::SettingsMenu,
		shared::Shared,
	},
	app_error::Context,
	args::Args,
	cgmath::Point2,
	chrono::TimeDelta,
	clap::Parser,
	crossbeam::atomic::AtomicCell,
	directories::ProjectDirs,
	futures::{Future, StreamExt, TryStreamExt, lock::Mutex, stream::FuturesUnordered},
	std::{collections::HashMap, sync::Arc},
	tokio::fs,
	winit::{
		dpi::{PhysicalPosition, PhysicalSize},
		event::WindowEvent,
		event_loop::EventLoop,
		platform::{run_on_demand::EventLoopExtRunOnDemand, x11::EventLoopBuilderExtX11},
		window::{Window, WindowId},
	},
	zsw_egui::{EguiEventHandler, EguiPainter, EguiRenderer},
	zsw_util::{AppError, Rect, TokioTaskBlockOn, UnwrapOrReturnExt, WalkDir},
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
	std::fs::create_dir_all(dirs.data_dir()).context("Unable to create data directory")?;
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
	window_event_handlers: HashMap<WindowId, EguiEventHandler>,
	shared:                Arc<Shared>,
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
		match event {
			winit::event::WindowEvent::Resized(size) => self.shared.last_resize.store(Some(Resize { size })),
			winit::event::WindowEvent::CursorMoved { position, .. } => self.shared.cursor_pos.store(position),
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
		event_loop_proxy: winit::event_loop::EventLoopProxy<AppEvent>,
	) -> Result<Self, AppError> {
		let wgpu = Wgpu::new().await.context("Unable to initialize wgpu")?;
		let panels_renderer_layouts = PanelsRendererShared::new(&wgpu);

		let displays = Displays::new(config_dirs.displays().to_path_buf());
		let playlists = Playlists::new(config_dirs.playlists().to_path_buf());
		let profiles = Profiles::new(config_dirs.profiles().to_path_buf());

		// Shared state
		let shared = Shared {
			event_loop_proxy,
			last_resize: AtomicCell::new(None),
			// TODO: Not have a default of (0,0)?
			cursor_pos: AtomicCell::new(PhysicalPosition::new(0.0, 0.0)),
			wgpu,
			panels_renderer_shared: panels_renderer_layouts,
			displays: Arc::new(displays),
			playlists: Arc::new(playlists),
			profiles,
			panels: Mutex::new(vec![]),
		};
		let shared = Arc::new(shared);

		if let Some(profile) = &config.default.profile {
			let profile_name = ProfileName::from(profile.clone());
			let shared = Arc::clone(&shared);
			self::spawn_task("Load default profile", async move {
				self::load_profile(profile_name, &shared).await
			});
		}

		Ok(Self {
			window_event_handlers: HashMap::new(),
			shared,
		})
	}

	/// Initializes the window related things
	pub fn init_window(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) -> Result<(), AppError> {
		let windows = window::create(event_loop).context("Unable to create winit event loop and window")?;
		for app_window in windows {
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
			self::spawn_task("Renderer", async move {
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

/// Loads a profile
async fn load_profile(profile_name: ProfileName, shared: &Arc<Shared>) -> Result<(), AppError> {
	// Load the profile
	let profile = shared
		.profiles
		.load(profile_name)
		.await
		.context("Unable to load profile")?;

	// Then load it's panels
	profile
		.panels
		.iter()
		.map(async |profile_panel| {
			let display = shared
				.displays
				.load(profile_panel.display.clone())
				.await
				.with_context(|| format!("Unable to load display {:?}", profile_panel.display))?;


			let panel_state = match &profile_panel.shader {
				ProfilePanelShader::None(shader) => PanelState::None(PanelNoneState::new(shader.background_color)),
				ProfilePanelShader::Fade(shader) => {
					let state = PanelFadeState::new(shader.duration, shader.fade_duration, match shader.inner {
						ProfilePanelFadeShaderInner::Basic => PanelFadeShader::Basic,
						ProfilePanelFadeShaderInner::White { strength } => PanelFadeShader::White { strength },
						ProfilePanelFadeShaderInner::Out { strength } => PanelFadeShader::Out { strength },
						ProfilePanelFadeShaderInner::In { strength } => PanelFadeShader::In { strength },
					});

					let playlist_player = Arc::clone(state.playlist_player());

					let panel_playlists = shader.playlists.clone();

					#[cloned(playlists = shared.playlists)]
					self::spawn_task(
						format!("Load panel {:?} playlists", profile_panel.display),
						async move {
							panel_playlists
								.into_iter()
								.map(async |playlist_name| {
									self::load_playlist(&playlist_player, &playlist_name, &playlists)
										.await
										.with_context(|| format!("Unable to load playlist {playlist_name:?}"))
								})
								.collect::<FuturesUnordered<_>>()
								.try_collect::<()>()
								.await
						},
					);

					PanelState::Fade(state)
				},
			};

			let panel = Panel::new(&*display.lock().await, panel_state);
			shared.panels.lock().await.push(panel);

			Ok::<_, AppError>(())
		})
		.collect::<FuturesUnordered<_>>()
		.try_collect::<()>()
		.await?;

	Ok(())
}

/// Loads a panel's playlist
async fn load_playlist(
	playlist_player: &Mutex<PlaylistPlayer>,
	playlist: &PlaylistName,
	playlists: &Playlists,
) -> Result<(), AppError> {
	let playlist = playlists
		.load(playlist.clone())
		.await
		.context("Unable to load playlist")?;
	tracing::debug!("Loaded default playlist {playlist:?}");

	playlist
		.items
		.iter()
		.map(async |item| {
			// If not enabled, skip it
			if !item.enabled {
				return;
			}

			// Else check the kind of item
			match item.kind {
				PlaylistItemKind::Directory {
					path: ref dir_path,
					recursive,
				} =>
					WalkDir::builder()
						.max_depth(match recursive {
							true => None,
							false => Some(0),
						})
						.recurse_symlink(true)
						.build(dir_path.to_path_buf())
						.map(|entry| async {
							let entry = match entry {
								Ok(entry) => entry,
								Err(err) => {
									let err = AppError::new(&err);
									tracing::warn!("Unable to read directory entry: {}", err.pretty());
									return;
								},
							};

							let path = entry.path();
							if fs::metadata(&path)
								.await
								.map_err(|err| {
									let err = AppError::new(&err);
									tracing::warn!("Unable to get playlist entry {path:?} metadata: {}", err.pretty());
								})
								.unwrap_or_return()?
								.is_dir()
							{
								// If it's a directory, skip it
								return;
							}

							match tokio::fs::canonicalize(&path).await {
								Ok(entry) => playlist_player.lock().await.insert(entry.into()),
								Err(err) => {
									let err = AppError::new(&err);
									tracing::warn!("Unable to read playlist entry {path:?}: {}", err.pretty());
								},
							}
						})
						.collect::<FuturesUnordered<_>>()
						.await
						.collect::<()>()
						.await,

				PlaylistItemKind::File { ref path } => match tokio::fs::canonicalize(path).await {
					Ok(path) => playlist_player.lock().await.insert(path.into()),
					Err(err) => {
						let err = AppError::new(&err);
						tracing::warn!("Unable to canonicalize playlist entry {path:?}: {}", err.pretty());
					},
				},
			}
		})
		.collect::<FuturesUnordered<_>>()
		.collect::<()>()
		.await;


	Ok(())
}

/// Spawns a task
#[track_caller]
pub fn spawn_task<Fut>(name: impl Into<String>, fut: Fut)
where
	Fut: Future<Output = Result<(), AppError>> + Send + 'static,
{
	let name = name.into();

	#[cloned(name)]
	let fut = async move {
		let id = tokio::task::id();
		tracing::debug!("Spawning task {name:?} ({id:?})");
		fut.await
			.inspect(|()| tracing::debug!("Task {name:?} ({id:?}) finished"))
			.inspect_err(|err| tracing::warn!("Task {name:?} ({id:?}) returned error: {}", err.pretty()))
	};

	if let Err(err) = tokio::task::Builder::new().name(&name.clone()).spawn(fut) {
		let err = AppError::new(&err);
		tracing::warn!("Unable to spawn task {name:?}: {}", err.pretty());
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
		let (egui_paint_jobs, egui_textures_delta) =
			match self::paint_egui(shared, window, &egui_painter, &mut settings_menu, window_geometry).await {
				Ok((paint_jobs, textures_delta)) => (paint_jobs, Some(textures_delta)),
				Err(err) => {
					tracing::warn!("Unable to draw egui: {}", err.pretty());
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
				&mut frame,
				&wgpu_renderer,
				&shared.wgpu,
				&shared.panels_renderer_shared,
				&mut shared.panels.lock().await,
				window,
				window_geometry,
			)
			.await
			.context("Unable to render panels")?;

		// Render egui
		egui_renderer
			.render_egui(&mut frame, window, &shared.wgpu, &egui_paint_jobs, egui_textures_delta)
			.context("Unable to render egui")?;

		// Finish the frame
		if frame.finish(&shared.wgpu) {
			wgpu_renderer
				.reconfigure(&shared.wgpu)
				.context("Unable to reconfigure wgpu")?;
		}

		// Resize if we need to
		if let Some(resize) = shared.last_resize.swap(None) {
			wgpu_renderer
				.resize(&shared.wgpu, resize.size)
				.context("Unable to resize wgpu")?;
			panels_renderer.resize(&wgpu_renderer, &shared.wgpu, resize.size);
		}
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
	let cursor_pos = shared.cursor_pos.load().cast::<f32>().to_logical(scale_factor);

	let full_output_fut = egui_painter.draw(window, async |ctx| {
		// Draw the settings menu
		tokio::task::block_in_place(|| {
			settings_menu.draw(
				ctx,
				&shared.wgpu,
				&shared.displays,
				&mut shared.panels.lock().block_on(),
				&shared.event_loop_proxy,
				cursor_pos,
				window_geometry,
			);
		});

		// Then go through all panels checking for interactions with their geometries
		let cursor_pos = shared.cursor_pos.load();
		let cursor_pos = Point2::new(cursor_pos.x as i32, cursor_pos.y as i32);
		for panel in &mut *shared.panels.lock().await {
			// If we're over an egui area, or none of the geometries are underneath the cursor, skip the panel
			if ctx.is_pointer_over_area() ||
				!panel
					.geometries
					.iter()
					.any(|geometry| geometry.geometry_on(window_geometry).contains(cursor_pos))
			{
				continue;
			}

			// Pause any double-clicked panels
			if ctx.input(|input| input.pointer.button_double_clicked(egui::PointerButton::Primary)) {
				match &mut panel.state {
					panel::PanelState::None(_) => (),
					panel::PanelState::Fade(state) => state.toggle_paused(),
				}
				break;
			}

			// Skip any ctrl-clicked/middle clicked panels
			if ctx.input(|input| {
				(input.pointer.button_clicked(egui::PointerButton::Primary) && input.modifiers.ctrl) ||
					input.pointer.button_clicked(egui::PointerButton::Middle)
			}) {
				match &mut panel.state {
					panel::PanelState::None(_) => (),
					panel::PanelState::Fade(state) => state.skip(&shared.wgpu).await,
				}
				break;
			}

			// Scroll panels
			let scroll_delta = ctx.input(|input| input.smooth_scroll_delta.y);
			if scroll_delta != 0.0 {
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
						break;
					},
				}
			}
		}

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
