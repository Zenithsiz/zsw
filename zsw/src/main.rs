//! Zenithsiz's scrolling wallpaper

#![feature(
	never_type,
	must_not_suspend,
	proc_macro_hygiene,
	stmt_expr_attributes,
	bool_toggle,
	oneshot_channel,
	str_as_str,
	unwrap_infallible,
	share_trait,
	duration_integer_division,
	arbitrary_self_types
)]

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
		renderer::WindowRenderer,
	},
	app_error::Context,
	clap::Parser,
	core::{cell::LazyCell, ptr::NonNull},
	directories::ProjectDirs,
	euclid::default::Vector2D,
	pollster::FutureExt,
	smithay_client_toolkit::shell::{WaylandSurface, wlr_layer::LayerSurface},
	std::{
		collections::{BTreeMap, HashMap},
		fs,
		process::ExitCode,
		sync::Arc,
	},
	wayland_client::{Connection, Proxy, backend::ObjectId, protocol::wl_surface::WlSurface},
	wgpu::rwh::{RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle},
	zsw_egui::EguiWaylandState,
	zsw_util::AppError,
	zsw_wayland::{WaylandApp, WaylandData, WaylandEventLoop, WaylandState},
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

		layers: HashMap::new(),
	};

	let mut wayland_event_loop = WaylandEventLoop::new().context("Unable to create wayland event loop")?;
	let wayland_data = WaylandData::new(&wayland_event_loop).context("Unable to create wayland")?;
	let mut wayland_state = WaylandState {
		app:  zsw,
		data: wayland_data,
	};

	let mut surface_ids = vec![];
	while !wayland_state.data.should_quit {
		// Dispatch events before rendering
		wayland_event_loop.dispatch(&mut wayland_state)?;

		surface_ids.extend(wayland_state.app.layers.keys().cloned());
		for surface_id in surface_ids.drain(..) {
			// Wait until the next frame
			// TODO: If we have two layers at different refresh rates, this will
			//       cause us to be limited to the slowest one. We need to manually
			//       sleep.
			let Some(layer) = wayland_state.app.layers.get_mut(&surface_id) else {
				continue;
			};
			let frame = match &mut layer.renderer {
				Some(renderer) => Some(renderer.wait_frame().context("Unable to start new frame")?),
				None => None,
			};

			// Also dispatch events after waiting for the frame to start.
			// Note: We do this now to ensure that the frame operates on the latest
			//       data, instead of data that's potentially 1 frame late.
			wayland_event_loop.dispatch(&mut wayland_state)?;

			// Finally render
			let Some(layer) = wayland_state.app.layers.get_mut(&surface_id) else {
				continue;
			};
			if let Some(renderer) = &mut layer.renderer &&
				let Some(frame) = frame
			{
				let egui_input = layer.egui_state.take_input();
				let egui_output = renderer
					.render(
						&mut wayland_state.data,
						&wayland_state.app.playlists,
						&wayland_state.app.profiles,
						egui_input,
						frame,
					)
					.context("Unable to render frame")?;

				layer
					.egui_state
					.update_output(&mut wayland_event_loop, &mut wayland_state.data, egui_output);
			}
		}
	}

	Ok(())
}

struct ZswLayer {
	renderer:   Option<WindowRenderer>,
	egui_state: EguiWaylandState,
}

struct Zsw {
	playlists:    Playlists,
	profiles:     Profiles,
	profile_name: ProfileName,

	layers: HashMap<SurfaceId, ZswLayer>,
}

impl WaylandApp for Zsw {
	fn configure_layer(
		&mut self,
		_data: &mut WaylandData<Self>,
		conn: &Connection,
		layer: &LayerSurface,
		surface_size: Vector2D<u32>,
	) {
		let surface_id = layer.wl_surface().id();
		let layer = self
			.layers
			.entry(SurfaceId(surface_id.clone()))
			.or_insert_with(|| ZswLayer {
				renderer:   None,
				egui_state: EguiWaylandState::new(),
			});

		match &mut layer.renderer {
			Some(renderer) => {
				tracing::info!(size=?surface_size, "Resizing renderer");
				renderer.queue_resize(surface_size);
			},
			None => {
				tracing::info!(size=?surface_size, "Creating renderer window");

				let display_ptr = NonNull::new(conn.backend().display_ptr().cast()).expect("Display was null");
				let raw_display_handle = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(display_ptr));
				let surface_ptr = NonNull::new(surface_id.as_ptr().cast()).expect("Surface was null");
				let raw_window_handle = RawWindowHandle::Wayland(WaylandWindowHandle::new(surface_ptr));
				let target = wgpu::SurfaceTargetUnsafe::RawHandle {
					raw_display_handle: Some(raw_display_handle),
					raw_window_handle,
				};
				// SAFETY: The window is only dropped after wgpu.
				let target = unsafe { zsw_wgpu::SurfaceTarget::from_wgpu_unsafe(target) };

				match WindowRenderer::new(
					target,
					surface_size,
					&self.profiles,
					&self.profile_name,
					&self.playlists,
				)
				.block_on()
				{
					Ok(renderer) => {
						layer.egui_state.update_wgpu(renderer.wgpu_renderer());
						layer.renderer = Some(renderer);
					},
					Err(err) => {
						tracing::error!("Unable to create window: {err:?}");
					},
				}
			},
		}

		layer.egui_state.update_surface_size(surface_size);
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
		for layer in self.layers.values_mut() {
			layer
				.egui_state
				.update_keyboard_key(data, keysym, raw, text.clone(), state);
		}
	}

	fn on_keyboard_focus(&mut self, _data: &mut zsw_wayland::WaylandData<Self>, surface: &WlSurface, focused: bool) {
		let Some(layer) = self.layers.get_mut(&SurfaceId(surface.id())) else {
			tracing::warn!(surface=%surface.id(), "Received keyboard focus for unknown surface");
			return;
		};

		layer.egui_state.update_keyboard_focus(focused);
	}

	fn on_keyboard_modifiers(
		&mut self,
		_data: &mut zsw_wayland::WaylandData<Self>,
		modifiers: smithay_client_toolkit::seat::keyboard::Modifiers,
		raw_modifiers: smithay_client_toolkit::seat::keyboard::RawModifiers,
	) {
		#[expect(clippy::iter_over_hash_type, reason = "Order doesn't matter")]
		for layer in self.layers.values_mut() {
			layer.egui_state.update_keyboard_modifiers(modifiers, raw_modifiers);
		}
	}

	fn on_pointer(
		&mut self,
		_data: &mut zsw_wayland::WaylandData<Self>,
		events: &[smithay_client_toolkit::seat::pointer::PointerEvent],
	) {
		for event in events {
			let Some(layer) = self.layers.get_mut(&SurfaceId(event.surface.id())) else {
				tracing::warn!(?event, "Received event for unknown surface");
				continue;
			};

			layer.egui_state.update_pointer(event);
		}
	}
}

/// Surface Id
#[derive(PartialEq, Eq, Clone, Hash, Debug)]
pub struct SurfaceId(pub ObjectId);
