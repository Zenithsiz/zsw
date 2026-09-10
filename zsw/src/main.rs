//! Zenithsiz's scrolling wallpaper

#![feature(
	stmt_expr_attributes,
	oneshot_channel,
	str_as_str,
	unwrap_infallible,
	share_trait,
	duration_integer_division,
	arbitrary_self_types,
	thread_sleep_until,
	try_entry
)]
#![recursion_limit = "256"]

mod args;
mod config;
mod dirs;
mod menu;
mod panel;
mod playlist;
mod profile;
mod renderer;

use {
	self::{
		args::Args,
		config::Config,
		dirs::Dirs,
		playlist::Playlists,
		profile::{Profile, ProfileName, Profiles},
		renderer::SurfaceRenderer,
	},
	app_error::Context,
	clap::Parser,
	core::{cell::LazyCell, ptr::NonNull, time::Duration},
	directories::ProjectDirs,
	euclid::default::Vector2D,
	pollster::FutureExt,
	smithay_client_toolkit::shell::{WaylandSurface, wlr_layer::LayerSurface},
	std::{
		collections::{BTreeMap, HashMap},
		fs,
		process::ExitCode,
		sync::Arc,
		thread,
		time::Instant,
	},
	wayland_client::{Connection, Proxy, protocol::wl_surface::WlSurface},
	wgpu::rwh::{RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle},
	zsw_egui::EguiWaylandState,
	zsw_util::AppError,
	zsw_wayland::{WaylandApp, WaylandData, WaylandEventLoop, WaylandState, data::SurfaceId},
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

fn run() -> Result<(), AppError> {
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

	let playlists = zsw_util::read_dir_all_toml(dirs.playlists()).context("Unable to create playlists")?;
	let profiles = zsw_util::read_dir_all_toml::<_, Arc<Profile>, BTreeMap<_, _>>(dirs.profiles())
		.context("Unable to create profiles")?;

	let zsw = Zsw {
		playlists,
		profiles,
		profile_name: args.profile,

		surfaces: HashMap::new(),
	};

	let mut wayland_event_loop = WaylandEventLoop::new().context("Unable to create wayland event loop")?;
	let wayland_data = WaylandData::new(&wayland_event_loop).context("Unable to create wayland")?;
	let mut wayland_state = WaylandState {
		app:  zsw,
		data: wayland_data,
	};

	while !wayland_state.data.should_quit {
		// Get the surface we need to render next
		let Some((surface_id, surface)) = wayland_state
			.app
			.surfaces
			.iter()
			.min_by_key(|(_, surface)| surface.next_frame)
		else {
			// Note: If we have no surfaces, just spin on the dispatch loop until we do.
			// TODO: This could spin at 100% CPU if the dispatch fails configuring the
			//       surface on all layers, should we maybe quit after a while, or sleep
			//       in between dispatch calls?
			wayland_event_loop.dispatch(&mut wayland_state)?;
			continue;
		};
		let surface_id = surface_id.clone();
		let _span = tracing::trace_span!("render", %surface_id).entered();

		// Sleep and then dispatch events, *in that order*
		// Note: We dispatch the events after sleeping to ensure that the frame operates
		//       on the latest data, instead of data that's potentially 1 frame late.
		//       The dispatch should be quick, so this doesn't cost us much time in the
		//       frame.
		thread::sleep_until(surface.next_frame);
		wayland_event_loop.dispatch(&mut wayland_state)?;

		// Finally render
		// Note: It's possible for the surface to have disappeared during the dispatch loop,
		//       in which case we can just go next.
		let Some(surface) = wayland_state.app.surfaces.get_mut(&surface_id) else {
			continue;
		};
		let Some(renderer) = &mut surface.renderer else {
			continue;
		};

		let mut frame = renderer.start_frame().context("Unable to start new frame")?;
		let egui_input = surface.egui_state.take_input();
		let egui_output = renderer
			.render(
				&mut wayland_state.data,
				&wayland_state.app.playlists,
				&wayland_state.app.profiles,
				egui_input,
				&mut frame,
			)
			.context("Unable to render frame")?;

		surface
			.egui_state
			.update_output(&mut wayland_event_loop, &mut wayland_state.data, egui_output);

		let frame = renderer.submit_frame(frame).context("Unable to submit frame")?;
		renderer.present_frame(frame).context("Unable to present frame")?;

		let now = Instant::now();
		tracing::trace!("Frame took {:?}", now - surface.last_frame);
		surface.last_frame = now;
		surface.next_frame += surface.frame_duration;
		if let Some(late) = now.checked_duration_since(surface.next_frame) {
			tracing::trace!("Frame was {late:?} late, skipping frames");
			surface.next_frame = now;
		}
	}

	Ok(())
}

struct ZswSurface {
	renderer:   Option<SurfaceRenderer>,
	egui_state: EguiWaylandState,

	last_frame:     Instant,
	next_frame:     Instant,
	frame_duration: Duration,
}

struct Zsw {
	playlists:    Playlists,
	profiles:     Profiles,
	profile_name: ProfileName,

	surfaces: HashMap<SurfaceId, ZswSurface>,
}

impl WaylandApp for Zsw {
	fn configure_layer(
		&mut self,
		data: &mut WaylandData<Self>,
		conn: &Connection,
		layer: &LayerSurface,
		surface_size: Vector2D<u32>,
	) {
		let surface_id = SurfaceId(layer.wl_surface().id());
		let Ok(surface) = self.surfaces.entry(surface_id.clone()).or_try_insert_with(|| {
			let Some(layer_data) = data.layers.iter().find(|layer| layer.surface_id == surface_id) else {
				tracing::warn!(%surface_id, "Unable to find layer data with surface id");
				return Err(());
			};

			let Some(output_mode) = layer_data.output_info.modes.iter().find(|mode| mode.current) else {
				tracing::warn!(modes=?layer_data.output_info.modes, "Unable to find current mode for layer");
				return Err(());
			};

			tracing::info!(
				"Found refresh rate for surface {surface_id}: {:.3}Hz",
				output_mode.refresh_rate as f32 / 1000.0
			);
			let frame_duration = match u32::try_from(output_mode.refresh_rate) {
				Ok(0) | Err(_) => {
					tracing::warn!("Cannot use a non-positive refresh rate, using 60Hz instead");
					Duration::from_secs_f32(1.0 / 60.0)
				},
				Ok(refresh_rate) => Duration::from_secs(1000) / refresh_rate,
			};

			let now = Instant::now();
			Ok(ZswSurface {
				renderer: None,
				egui_state: EguiWaylandState::new(),

				last_frame: now,
				next_frame: now,
				frame_duration,
			})
		}) else {
			return;
		};

		match &mut surface.renderer {
			Some(renderer) => {
				tracing::info!(size=?surface_size, "Resizing renderer");
				renderer.queue_resize(surface_size);
			},
			None => {
				tracing::info!(size=?surface_size, "Creating renderer window");

				let display_ptr = NonNull::new(conn.backend().display_ptr().cast()).expect("Display was null");
				let raw_display_handle = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(display_ptr));
				let surface_ptr = NonNull::new(surface_id.0.as_ptr().cast()).expect("Surface was null");
				let raw_window_handle = RawWindowHandle::Wayland(WaylandWindowHandle::new(surface_ptr));
				let target = wgpu::SurfaceTargetUnsafe::RawHandle {
					raw_display_handle: Some(raw_display_handle),
					raw_window_handle,
				};
				// SAFETY: The window is only dropped after wgpu.
				let target = unsafe { zsw_wgpu::SurfaceTarget::from_wgpu_unsafe(target) };

				match SurfaceRenderer::new(
					target,
					surface_size,
					&self.profiles,
					&self.profile_name,
					&self.playlists,
				)
				.block_on()
				{
					Ok(renderer) => {
						surface.egui_state.update_wgpu(renderer.wgpu_renderer());
						surface.renderer = Some(renderer);
					},
					Err(err) => tracing::error!("Unable to create surface renderer: {err:?}"),
				}
			},
		}

		surface.egui_state.update_surface_size(surface_size);
	}

	fn on_keyboard_key(
		&mut self,
		data: &mut zsw_wayland::WaylandData<Self>,
		keysym: xkeysym::Keysym,
		raw: u32,
		text: Option<String>,
		state: zsw_wayland::KeyboardKeyState,
	) {
		#[expect(clippy::iter_over_hash_type, reason = "Order doesn't matter")]
		for surface in self.surfaces.values_mut() {
			surface
				.egui_state
				.update_keyboard_key(data, keysym, raw, text.clone(), state);
		}
	}

	fn on_keyboard_focus(&mut self, _data: &mut zsw_wayland::WaylandData<Self>, surface: &WlSurface, focused: bool) {
		let Some(surface) = self.surfaces.get_mut(&SurfaceId(surface.id())) else {
			tracing::warn!(surface=%surface.id(), "Received keyboard focus for unknown surface");
			return;
		};

		surface.egui_state.update_keyboard_focus(focused);
	}

	fn on_keyboard_modifiers(
		&mut self,
		_data: &mut zsw_wayland::WaylandData<Self>,
		modifiers: smithay_client_toolkit::seat::keyboard::Modifiers,
		raw_modifiers: smithay_client_toolkit::seat::keyboard::RawModifiers,
	) {
		#[expect(clippy::iter_over_hash_type, reason = "Order doesn't matter")]
		for surface in self.surfaces.values_mut() {
			surface.egui_state.update_keyboard_modifiers(modifiers, raw_modifiers);
		}
	}

	fn on_pointer(
		&mut self,
		_data: &mut zsw_wayland::WaylandData<Self>,
		events: &[smithay_client_toolkit::seat::pointer::PointerEvent],
	) {
		for event in events {
			let Some(surface) = self.surfaces.get_mut(&SurfaceId(event.surface.id())) else {
				tracing::warn!(?event, "Received event for unknown surface");
				continue;
			};

			surface.egui_state.update_pointer(event);
		}
	}
}
