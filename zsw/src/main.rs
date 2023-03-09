//! Zenithsiz's scrolling wallpaper

// TODO: When using mailbox (uncapped FPS), a deadlock occurs.
//       Strangely doesn't occur on immediate, might be a driver issue.

// Features
#![feature(
	never_type,
	decl_macro,
	result_option_inspect,
	async_closure,
	assert_matches,
	async_fn_in_trait,
	type_alias_impl_trait,
	impl_trait_projections,
	path_file_prefix,
	entry_insert,
	fs_try_exists,
	let_chains,
	exit_status_error,
	lint_reasons,
	closure_track_caller,
	generic_const_exprs,
	once_cell,
	return_position_impl_trait_in_trait,
	associated_type_bounds
)]
#![allow(incomplete_features)]

// Modules
mod args;
mod config;
mod egui_wrapper;
mod image_loader;
mod logger;
mod panel;
mod playlist;
mod rayon_init;
mod settings_menu;
mod shared;
mod tokio_runtime;
mod wgpu_wrapper;
mod window;

// Imports
use {
	self::{
		config::Config,
		egui_wrapper::{EguiPainter, EguiRenderer},
		panel::{PanelShader, PanelsManager, PanelsRenderer},
		settings_menu::SettingsMenu,
		shared::{
			AsyncMutexResource,
			AsyncRwLockResource,
			CurPanelGroupMutex,
			EguiPainterRendererMeetupSender,
			Locker,
			MeetupSenderResource,
			PanelsRendererShaderRwLock,
			PanelsUpdaterMeetupSender,
			PlaylistsRwLock,
			Shared,
		},
		wgpu_wrapper::WgpuRenderer,
	},
	anyhow::Context,
	args::Args,
	cgmath::Point2,
	clap::Parser,
	crossbeam::atomic::AtomicCell,
	directories::ProjectDirs,
	futures::Future,
	std::{mem, sync::Arc},
	winit::{
		dpi::{PhysicalPosition, PhysicalSize},
		platform::run_return::EventLoopExtRunReturn,
	},
	zsw_util::meetup,
};


fn main() -> Result<(), anyhow::Error> {
	// Get arguments
	let args = Args::parse();
	logger::pre_init::debug(format!("args: {args:?}"));

	// Create the configuration then load the config
	let dirs = ProjectDirs::from("", "", "zsw").context("Unable to create app directories")?;
	std::fs::create_dir_all(dirs.data_dir()).context("Unable to create data directory")?;
	let config_path = args.config.unwrap_or_else(|| dirs.data_dir().join("config.yaml"));
	let config = Config::get_or_create_default(&config_path);
	logger::pre_init::debug(format!("config_path: {config_path:?}, config: {config:?}"));

	// Initialize the logger properly now
	logger::init(args.log_file.as_deref().or(config.log_file.as_deref()));

	// Initialize and create everything
	rayon_init::init(config.rayon_worker_threads).context("Unable to initialize rayon")?;
	let tokio_runtime = tokio_runtime::create(config.tokio_worker_threads).context("Unable to create tokio runtime")?;

	// Then run `run` on the tokio runtime
	let _runtime_enter = tokio_runtime.enter();
	tokio_runtime.block_on(self::run(&dirs, &config))?;

	Ok(())
}

#[cfg(feature = "include-shaders")]
static SHADERS_DIR: include_dir::Dir<'_> = include_dir::include_dir!("shaders/");

#[allow(clippy::too_many_lines)] // TODO: Separate
async fn run(dirs: &ProjectDirs, config: &Config) -> Result<(), anyhow::Error> {
	let (mut event_loop, window) = window::create().context("Unable to create winit event loop and window")?;
	let window = Arc::new(window);
	let (wgpu_shared, wgpu_renderer) = wgpu_wrapper::create(Arc::clone(&window))
		.await
		.context("Unable to create wgpu renderer")?;

	let shaders_path = config
		.shaders_dir
		.clone()
		.unwrap_or_else(|| dirs.data_dir().join("shaders/"));

	// If the shaders path doesn't exist, write it
	// TODO: Use a virtual filesystem instead?
	if !std::fs::try_exists(&shaders_path).context("Unable to check if shaders path exists")? {
		#[cfg(feature = "include-shaders")]
		SHADERS_DIR
			.extract(&shaders_path)
			.context("Unable to extract shaders directory")?;

		#[cfg(not(feature = "include-shaders"))]
		tracing::warn!("Shaders directory doesn't exist on the filesystem and not included in the binary");
	}

	let (panels_renderer, panels_renderer_layout, panels_renderer_shader) =
		PanelsRenderer::new(&wgpu_renderer, &wgpu_shared, shaders_path.join("panels/fade.wgsl"))
			.context("Unable to create panels renderer")?;
	let (egui_renderer, egui_painter, mut egui_event_handler) =
		egui_wrapper::create(&window, &wgpu_renderer, &wgpu_shared);
	let settings_menu = SettingsMenu::new();

	let playlist_path = config
		.playlists_dir
		.clone()
		.unwrap_or_else(|| dirs.data_dir().join("playlists/"));
	let (playlists_manager, playlists) = playlist::create(playlist_path)
		.await
		.context("Unable to create playlist manager")?;

	let panels_path = config
		.panels_dir
		.clone()
		.unwrap_or_else(|| dirs.data_dir().join("panels/"));
	let panels_manager = PanelsManager::new(panels_path);

	let upscale_cache_dir = config
		.upscale_cache_dir
		.clone()
		.unwrap_or_else(|| dirs.data_dir().join("upscale_cache/"));
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
		panels_manager,
		image_requester,
		playlists_manager,
		cur_panel_group: CurPanelGroupMutex::new(None),
		panels_renderer_shader: PanelsRendererShaderRwLock::new(panels_renderer_shader),
		playlists: PlaylistsRwLock::new(playlists),
	};
	let shared = Arc::new(shared);

	let (egui_painter_output_tx, egui_painter_output_rx) = meetup::channel();
	let (panels_updater_output_tx, panels_updater_output_rx) = meetup::channel();


	self::spawn_task("Load default panel group", {
		let shared = Arc::clone(&shared);
		let locker = Locker::new();
		let default_panel_group = config.default_panel_group.clone();
		async move { self::load_default_panel_group(default_panel_group, locker, shared).await }
	});

	self::spawn_task("Renderer", {
		let shared = Arc::clone(&shared);
		let locker = Locker::new();
		async move {
			self::renderer(
				shared,
				locker,
				wgpu_renderer,
				panels_renderer,
				egui_renderer,
				egui_painter_output_rx,
				panels_updater_output_rx,
			)
			.await
		}
	});

	self::spawn_task("Panels updater", {
		let shared = Arc::clone(&shared);
		let locker = Locker::new();
		let panels_updater_output_tx = PanelsUpdaterMeetupSender::new(panels_updater_output_tx);
		async move { self::panels_updater(shared, locker, panels_updater_output_tx).await }
	});

	self::spawn_task("Image loader", async move { image_loader.run().await });

	self::spawn_task("Egui painter", {
		let shared = Arc::clone(&shared);
		let locker = Locker::new();
		let egui_painter_output_tx = EguiPainterRendererMeetupSender::new(egui_painter_output_tx);
		async move { self::egui_painter(shared, locker, egui_painter, settings_menu, egui_painter_output_tx).await }
	});

	// Then run the event loop on this thread
	let _ = tokio::task::block_in_place(|| {
		event_loop.run_return(|event, _, control_flow| {
			*control_flow = winit::event_loop::ControlFlow::Wait;

			#[allow(clippy::single_match)] // We'll add more in the future
			match event {
				winit::event::Event::WindowEvent { ref event, .. } => match *event {
					winit::event::WindowEvent::Resized(size) => shared.last_resize.store(Some(Resize { size })),
					winit::event::WindowEvent::CursorMoved { position, .. } => shared.cursor_pos.store(position),
					_ => (),
				},
				_ => (),
			}

			if let Some(event) = event.to_static() {
				egui_event_handler.handle_event(event);
			}
		})
	});

	Ok(())
}

/// Loads the default panel group
async fn load_default_panel_group(
	default_panel_group: Option<String>,
	mut locker: Locker<'_, 0>,
	shared: Arc<Shared>,
) -> Result<(), anyhow::Error> {
	// If we don't have a default, don't do anything
	let Some(default_panel_group) = &default_panel_group else {
		return Ok(());
	};

	// Else load the panel group
	let loaded_panel_group = match shared
		.panels_manager
		.load(
			default_panel_group,
			&shared.wgpu,
			&shared.panels_renderer_layout,
			&shared.playlists,
			&mut locker,
		)
		.await
	{
		Ok(panel_group) => {
			tracing::debug!(?panel_group, "Loaded default panel group");
			panel_group
		},
		Err(err) => {
			tracing::warn!("Unable to load default panel group: {err:?}");
			return Ok(());
		},
	};

	// And set it as the current one
	let (mut panel_group, _) = shared.cur_panel_group.lock(&mut locker).await;
	*panel_group = Some(loaded_panel_group);

	mem::drop(panel_group);
	let (mut panels_renderer_shader, _) = shared.panels_renderer_shader.write(&mut locker).await;
	panels_renderer_shader.shader = PanelShader::FadeOut { strength: 1.5 };

	Ok(())
}

/// Spawns a task
#[track_caller]
pub fn spawn_task<F, T>(name: impl Into<String>, future: F)
where
	F: Future<Output = Result<T, anyhow::Error>> + Send + 'static,
{
	let name = name.into();

	#[allow(clippy::let_underscore_future)] // We don't care about the result
	let _ = tokio::task::Builder::new().name(&name.clone()).spawn(async move {
		tracing::debug!(?name, "Spawning task");
		match future.await {
			Ok(_) => tracing::debug!(?name, "Task finished"),
			Err(err) => tracing::debug!(?name, ?err, "Task returned error"),
		}
	});
}

/// Renderer task
async fn renderer(
	shared: Arc<Shared>,
	mut locker: Locker<'_, 0>,
	mut wgpu_renderer: WgpuRenderer,
	mut panels_renderer: PanelsRenderer,
	mut egui_renderer: EguiRenderer,
	egui_painter_output_rx: meetup::Receiver<(Vec<egui::ClippedPrimitive>, egui::TexturesDelta)>,
	panels_updater_output_rx: meetup::Receiver<()>,
) -> Result<!, anyhow::Error> {
	let mut egui_paint_jobs = vec![];
	let mut egui_textures_delta = None;
	loop {
		// Meetup with the panels updater
		let _ = panels_updater_output_rx.try_recv();

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
			let (mut panel_group, mut locker) = shared.cur_panel_group.lock(&mut locker).await;
			if let Some(panel_group) = &mut *panel_group {
				let cursor_pos = shared.cursor_pos.load();

				let (panels_renderer_shader, _) = shared.panels_renderer_shader.read(&mut locker).await;
				panels_renderer
					.render(
						&mut frame,
						&wgpu_renderer,
						&shared.wgpu,
						&shared.panels_renderer_layout,
						Point2::new(cursor_pos.x as i32, cursor_pos.y as i32),
						panel_group,
						&panels_renderer_shader,
					)
					.context("Unable to render panels")?;
			}
		}

		// Render egui
		egui_renderer
			.render_egui(
				&mut frame,
				&shared.window,
				&shared.wgpu,
				&egui_paint_jobs,
				egui_textures_delta.take(),
			)
			.context("Unable to render egui")?;

		// Finish the frame
		frame.finish(&shared.wgpu);

		// Resize if we need to
		if let Some(resize) = shared.last_resize.swap(None) {
			wgpu_renderer.resize(&shared.wgpu, resize.size);
			panels_renderer.resize(&wgpu_renderer, &shared.wgpu, resize.size);
		}
	}
}

/// Panel updater task
async fn panels_updater(
	shared: Arc<Shared>,
	mut locker: Locker<'_, 0>,
	panels_updater_output_tx: PanelsUpdaterMeetupSender,
) -> Result<!, anyhow::Error> {
	loop {
		{
			let (mut panel_group, _) = shared.cur_panel_group.lock(&mut locker).await;

			if let Some(panel_group) = &mut *panel_group {
				for panel in panel_group.panels_mut() {
					panel.update(&shared.wgpu, &shared.panels_renderer_layout, &shared.image_requester);
				}
			}
		}

		panels_updater_output_tx.send(&mut locker, ()).await;
	}
}

/// Egui painter task
async fn egui_painter(
	shared: Arc<Shared>,
	mut locker: Locker<'_, 0>,
	mut egui_painter: EguiPainter,
	mut settings_menu: SettingsMenu,
	egui_painter_output_tx: EguiPainterRendererMeetupSender,
) -> Result<!, anyhow::Error> {
	loop {
		let full_output = egui_painter.draw(&shared.window, |ctx, frame| {
			settings_menu.draw(ctx, frame, &shared, &mut locker);
			Ok::<_, !>(())
		})?;
		let paint_jobs = egui_painter.tessellate_shapes(full_output.shapes);
		let textures_delta = full_output.textures_delta;

		egui_painter_output_tx
			.send(&mut locker, (paint_jobs, textures_delta))
			.await;
	}
}

/// A resize
#[derive(Clone, Copy, Debug)]
pub struct Resize {
	/// New size
	size: PhysicalSize<u32>,
}
