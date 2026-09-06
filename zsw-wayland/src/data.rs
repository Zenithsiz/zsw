//! Wayland data

use {
	super::{WaylandApp, WaylandEventLoop, WaylandState},
	app_error::Context,
	calloop::LoopHandle,
	smithay_client_toolkit::{
		compositor::CompositorState,
		output::{OutputInfo, OutputState},
		registry::RegistryState,
		seat::{SeatState, pointer::ThemedPointer},
		shell::wlr_layer::{LayerShell, LayerSurface},
		shm::Shm,
	},
	smithay_clipboard::Clipboard,
	wayland_client::{backend::ObjectId, protocol::wl_keyboard::WlKeyboard},
	zsw_util::AppError,
};

/// Output Id
#[derive(PartialEq, Eq, Clone, Hash, Debug)]
#[derive(derive_more::Display)]
pub struct OutputId(pub ObjectId);

/// Surface Id
#[derive(PartialEq, Eq, Clone, Hash, Debug)]
#[derive(derive_more::Display)]
pub struct SurfaceId(pub ObjectId);

/// Wayland layer data
#[derive(Debug)]
pub struct WaylandLayerData {
	pub output_id:   OutputId,
	pub output_info: OutputInfo,

	pub surface_id:    SurfaceId,
	pub layer_surface: LayerSurface,
}

/// Wayland data
#[derive(derive_more::Debug)]
pub struct WaylandData<A> {
	pub loop_handle: LoopHandle<'static, WaylandState<A>>,

	pub registry_state: RegistryState,
	pub seat_state:     SeatState,
	pub output_state:   OutputState,

	pub compositor:  CompositorState,
	pub layer_shell: LayerShell,
	pub shm:         Shm,

	pub keyboard: Option<WlKeyboard>,
	pub pointer:  Option<ThemedPointer>,

	#[debug("..")]
	pub clipboard: Clipboard,

	pub layers: Vec<WaylandLayerData>,

	pub scale_factor: Option<i32>,

	pub should_quit: bool,
}

impl<A: WaylandApp> WaylandData<A> {
	/// Creates the wayland data
	pub fn new(event_loop: &WaylandEventLoop<A>) -> Result<Self, AppError> {
		let loop_handle = event_loop.event_loop.handle();

		let compositor =
			CompositorState::bind(&event_loop.globals, &event_loop.qh).context("Unable to create compositor")?;
		let layer_shell =
			LayerShell::bind(&event_loop.globals, &event_loop.qh).context("Unable to create layer shell")?;
		let shm = Shm::bind(&event_loop.globals, &event_loop.qh).context("Unable to create shm")?;

		let registry_state = RegistryState::new(&event_loop.globals);
		let seat_state = SeatState::new(&event_loop.globals, &event_loop.qh);
		let output_state = OutputState::new(&event_loop.globals, &event_loop.qh);

		// SAFETY: We pass in a valid display pointer
		let clipboard = unsafe { Clipboard::new(event_loop.conn.backend().display_ptr().cast()) };

		Ok(Self {
			loop_handle,

			registry_state,
			seat_state,
			output_state,

			compositor,
			layer_shell,
			shm,

			keyboard: None,
			pointer: None,

			clipboard,

			layers: vec![],
			scale_factor: None,

			should_quit: false,
		})
	}
}
