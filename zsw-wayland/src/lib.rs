//! Wayland

pub mod data;
pub mod event_loop;

pub use self::{
	data::{WaylandData, WaylandLayerData},
	event_loop::WaylandEventLoop,
};

use {
	self::data::{OutputId, SurfaceId},
	app_error::Context,
	core::iter,
	euclid::default::Vector2D,
	smithay_client_toolkit::{
		compositor::CompositorHandler,
		output::{OutputHandler, OutputState},
		registry::{ProvidesRegistryState, RegistryState},
		seat::{
			Capability,
			SeatHandler,
			SeatState,
			keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers},
			pointer::{PointerEvent, PointerHandler, ThemeSpec},
		},
		shell::{
			WaylandSurface,
			wlr_layer::{self, LayerShellHandler, LayerSurface, LayerSurfaceConfigure},
		},
		shm::{Shm, ShmHandler},
	},
	wayland_client::{
		Connection,
		Proxy,
		QueueHandle,
		protocol::{
			wl_keyboard::WlKeyboard,
			wl_output::{self, WlOutput},
			wl_pointer::WlPointer,
			wl_seat::WlSeat,
			wl_surface::WlSurface,
		},
	},
	zsw_util::AppError,
};

/// Wayland state
#[derive(Debug)]
pub struct WaylandState<A> {
	pub app:  A,
	pub data: WaylandData<A>,
}

impl<A: WaylandApp> WaylandState<A> {
	fn on_new_output(&mut self, _conn: &Connection, qh: &QueueHandle<Self>, output: &WlOutput) -> Result<(), AppError> {
		let output_id = OutputId(output.id());
		self.data.layers.retain(|layer| {
			if layer.output_id != output_id {
				return true;
			}

			tracing::warn!(%output_id, ?layer, "New output was created without destroying previous");
			false
		});

		let output_info = self
			.data
			.output_state
			.info(output)
			.context("Unable to get output information")?;

		tracing::info!(%output_id, "Creating layer on output");
		let surface = self.data.compositor.create_surface(qh);
		let layer_surface = self.data.layer_shell.create_layer_surface(
			qh,
			surface,
			wlr_layer::Layer::Background,
			Some("zsw"),
			Some(output),
		);
		layer_surface.set_anchor(wlr_layer::Anchor::all());
		layer_surface.set_keyboard_interactivity(wlr_layer::KeyboardInteractivity::OnDemand);
		layer_surface.commit();

		let layer = WaylandLayerData {
			output_id: OutputId(output.id()),
			output_info,
			surface_id: SurfaceId(layer_surface.wl_surface().id()),
			layer_surface,
		};
		self.data.layers.push(layer);

		Ok(())
	}

	fn on_destroy_output(
		&mut self,
		_conn: &Connection,
		_qh: &QueueHandle<Self>,
		output: &WlOutput,
	) -> Result<(), AppError> {
		self.data.layers.retain(|layer| layer.output_id.0 != output.id());

		Ok(())
	}

	fn on_new_keyboard(&mut self, qh: &QueueHandle<Self>, seat: &WlSeat) -> Result<(), AppError> {
		if self.data.keyboard.is_some() {
			tracing::info!(seat=%seat.id(), "Ignoring new keyboard");
			return Ok(());
		}

		let keyboard = self
			.data
			.seat_state
			.get_keyboard(qh, seat, None)
			.context("Unable to get keyboard")?;

		tracing::debug!(keyboard=%keyboard.id(), "Found keyboard");
		self.data.keyboard = Some(keyboard);

		Ok(())
	}

	fn on_new_pointer(&mut self, qh: &QueueHandle<Self>, seat: &WlSeat) -> Result<(), AppError> {
		if self.data.pointer.is_some() {
			tracing::info!(seat=%seat.id(), "Ignoring new pointer");
			return Ok(());
		}

		let surface = self.data.compositor.create_surface(qh);
		let pointer = self
			.data
			.seat_state
			.get_pointer_with_theme::<_, ()>(qh, seat, self.data.shm.wl_shm(), surface, ThemeSpec::System)
			.context("Unable to get pointer")?;

		tracing::debug!(pointer=%pointer.pointer().id(), "Found pointer");
		self.data.pointer = Some(pointer);

		Ok(())
	}
}

impl<A> CompositorHandler for WaylandState<A> {
	fn scale_factor_changed(
		&mut self,
		_conn: &Connection,
		_qh: &QueueHandle<Self>,
		_surface: &WlSurface,
		new_factor: i32,
	) {
		self.data.scale_factor = Some(new_factor);
	}

	fn transform_changed(
		&mut self,
		_conn: &Connection,
		_qh: &QueueHandle<Self>,
		_surface: &WlSurface,
		_new_transform: wl_output::Transform,
	) {
	}

	fn frame(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _surface: &WlSurface, _time: u32) {}

	fn surface_enter(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _surface: &WlSurface, _output: &WlOutput) {
	}

	fn surface_leave(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _surface: &WlSurface, _output: &WlOutput) {
	}
}

impl<A: WaylandApp> OutputHandler for WaylandState<A> {
	fn output_state(&mut self) -> &mut OutputState {
		&mut self.data.output_state
	}

	fn new_output(&mut self, conn: &Connection, qh: &QueueHandle<Self>, output: WlOutput) {
		let output_id = output.id();
		if let Err(err) = self.on_new_output(conn, qh, &output) {
			tracing::warn!(%output_id,"Unable to process output: {err:?}");
		}
	}

	fn update_output(&mut self, conn: &Connection, qh: &QueueHandle<Self>, output: WlOutput) {
		// TODO: Could we do better than this?
		let output_id = output.id();
		if let Err(err) = self.on_destroy_output(conn, qh, &output) {
			tracing::warn!(%output_id,"Unable to destroy output: {err:?}");
		}
		if let Err(err) = self.on_new_output(conn, qh, &output) {
			tracing::warn!(%output_id,"Unable to process output: {err:?}");
		}
	}

	fn output_destroyed(&mut self, conn: &Connection, qh: &QueueHandle<Self>, output: WlOutput) {
		let output_id = output.id();
		if let Err(err) = self.on_destroy_output(conn, qh, &output) {
			tracing::warn!(%output_id,"Unable to destroy output: {err:?}");
		}
	}
}

impl<A: WaylandApp> SeatHandler for WaylandState<A> {
	fn seat_state(&mut self) -> &mut SeatState {
		&mut self.data.seat_state
	}

	fn new_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: WlSeat) {}

	fn new_capability(&mut self, _conn: &Connection, qh: &QueueHandle<Self>, seat: WlSeat, capability: Capability) {
		match capability {
			Capability::Keyboard if self.data.keyboard.is_none() =>
				if let Err(err) = self.on_new_keyboard(qh, &seat) {
					tracing::warn!("Error while setting up keyboard: {err:?}");
				},
			Capability::Pointer if self.data.pointer.is_none() =>
				if let Err(err) = self.on_new_pointer(qh, &seat) {
					tracing::warn!("Error while setting up pointer: {err:?}");
				},
			_ => tracing::debug!(?seat, ?capability, "Ignoring new capability"),
		}
	}

	fn remove_capability(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, seat: WlSeat, capability: Capability) {
		// TODO: Check if the seat matches too?
		match capability {
			Capability::Keyboard if let Some(keyboard) = self.data.keyboard.take() => keyboard.release(),
			Capability::Pointer if let Some(pointer) = self.data.pointer.take() => {
				// TODO: Should we release the surface too?
				pointer.pointer().release();
			},
			_ => tracing::debug!(?seat, ?capability, "Ignoring removed capability"),
		}
	}

	fn remove_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: WlSeat) {}
}

impl<A: WaylandApp> KeyboardHandler for WaylandState<A> {
	fn enter(
		&mut self,
		_conn: &Connection,
		_qh: &QueueHandle<Self>,
		_keyboard: &WlKeyboard,
		surface: &WlSurface,
		_serial: u32,
		raw: &[u32],
		key_syms: &[Keysym],
	) {
		for (&keysym, &raw) in iter::zip(key_syms, raw) {
			// TODO: Should we manually try to get the string of the key here?
			self.app
				.on_keyboard_key(&mut self.data, keysym, raw, None, KeyboardKeyState::Pressed);
		}

		self.app.on_keyboard_focus(&mut self.data, surface, true);
	}

	fn leave(
		&mut self,
		_conn: &Connection,
		_qh: &QueueHandle<Self>,
		_keyboard: &WlKeyboard,
		surface: &WlSurface,
		_serial: u32,
	) {
		self.app.on_keyboard_focus(&mut self.data, surface, false);
	}

	fn press_key(
		&mut self,
		_conn: &Connection,
		_qh: &QueueHandle<Self>,
		_keyboard: &WlKeyboard,
		_serial: u32,
		event: KeyEvent,
	) {
		self.app.on_keyboard_key(
			&mut self.data,
			event.keysym,
			event.raw_code,
			event.utf8,
			KeyboardKeyState::Pressed,
		);
	}

	fn repeat_key(
		&mut self,
		_conn: &Connection,
		_qh: &QueueHandle<Self>,
		_keyboard: &WlKeyboard,
		_serial: u32,
		event: KeyEvent,
	) {
		self.app.on_keyboard_key(
			&mut self.data,
			event.keysym,
			event.raw_code,
			event.utf8,
			KeyboardKeyState::Repeat,
		);
	}

	fn release_key(
		&mut self,
		_conn: &Connection,
		_qh: &QueueHandle<Self>,
		_keyboard: &WlKeyboard,
		_serial: u32,
		event: KeyEvent,
	) {
		self.app.on_keyboard_key(
			&mut self.data,
			event.keysym,
			event.raw_code,
			event.utf8,
			KeyboardKeyState::Release,
		);
	}

	fn update_modifiers(
		&mut self,
		_conn: &Connection,
		_qh: &QueueHandle<Self>,
		_keyboard: &WlKeyboard,
		_serial: u32,
		modifiers: Modifiers,
		raw_modifiers: RawModifiers,
		_layout: u32,
	) {
		self.app.on_keyboard_modifiers(&mut self.data, modifiers, raw_modifiers);
	}
}

impl<A: WaylandApp> PointerHandler for WaylandState<A> {
	fn pointer_frame(
		&mut self,
		_conn: &Connection,
		_qh: &QueueHandle<Self>,
		_pointer: &WlPointer,
		events: &[PointerEvent],
	) {
		self.app.on_pointer(&mut self.data, events);
	}
}

impl<A: WaylandApp> LayerShellHandler for WaylandState<A> {
	fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, layer: &LayerSurface) {
		// TODO: We should maybe only close the specified layer instead of quitting everything?
		tracing::info!(surface=%layer.wl_surface().id(), "Received close request for layer");
		self.data.should_quit = true;
	}

	fn configure(
		&mut self,
		conn: &Connection,
		_qh: &QueueHandle<Self>,
		layer: &LayerSurface,
		configure: LayerSurfaceConfigure,
		_serial: u32,
	) {
		tracing::info!(?configure, "Configuring layer");

		let surface_size = euclid::vec2(configure.new_size.0, configure.new_size.1);
		self.app.configure_layer(&mut self.data, conn, layer, surface_size);
	}
}

impl<A> ShmHandler for WaylandState<A> {
	fn shm_state(&mut self) -> &mut Shm {
		&mut self.data.shm
	}
}

smithay_client_toolkit::delegate_registry!(@<A: WaylandApp> WaylandState<A>);

impl<A: WaylandApp> ProvidesRegistryState for WaylandState<A> {
	smithay_client_toolkit::registry_handlers![OutputState, SeatState];

	fn registry(&mut self) -> &mut RegistryState {
		&mut self.data.registry_state
	}
}

smithay_client_toolkit::delegate_dispatch2!(@<A> WaylandState<A>);

/// Wayland app
pub trait WaylandApp: Sized + 'static {
	/// Called when the wayland layer is configure
	fn configure_layer(
		&mut self,
		data: &mut WaylandData<Self>,
		conn: &Connection,
		layer: &LayerSurface,
		surface_size: Vector2D<u32>,
	);

	/// Called on keyboard presses
	fn on_keyboard_key(
		&mut self,
		data: &mut WaylandData<Self>,
		keysym: Keysym,
		raw: u32,
		text: Option<String>,
		state: KeyboardKeyState,
	);

	/// Called on keyboard focus
	fn on_keyboard_focus(&mut self, data: &mut WaylandData<Self>, surface: &WlSurface, focused: bool);

	/// Called on keyboard modifier update
	fn on_keyboard_modifiers(
		&mut self,
		data: &mut WaylandData<Self>,
		modifiers: Modifiers,
		raw_modifiers: RawModifiers,
	);

	/// Called on pointer update
	fn on_pointer(&mut self, data: &mut WaylandData<Self>, events: &[PointerEvent]);
}

/// Keyboard key state
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum KeyboardKeyState {
	Pressed,
	Repeat,
	Release,
}
