//! Wayland event loop

use {
	super::{WaylandApp, WaylandState},
	app_error::Context,
	calloop::{EventLoop, RegistrationToken},
	calloop_wayland_source::WaylandSource,
	core::time::Duration,
	wayland_client::{Connection, QueueHandle, globals::GlobalList},
	zsw_util::AppError,
};


/// Wayland event loop
#[derive(Debug)]
pub struct WaylandEventLoop<A> {
	pub(super) conn:    Connection,
	pub(super) globals: GlobalList,
	pub(super) qh:      QueueHandle<WaylandState<A>>,

	pub(super) event_loop: EventLoop<'static, WaylandState<A>>,
	pub(super) _token:     RegistrationToken,
}

impl<A: WaylandApp> WaylandEventLoop<A> {
	/// Creates the wayland event loop
	pub fn new() -> Result<Self, AppError> {
		let conn = wayland_client::Connection::connect_to_env().context("Unable to connect to wayland server")?;

		let (globals, event_queue) =
			wayland_client::globals::registry_queue_init(&conn).context("Unable to initialize registry queue")?;
		let qh = event_queue.handle();
		let event_loop = calloop::EventLoop::try_new().context("Unable to create event loop")?;

		let loop_handle = event_loop.handle();
		let token = WaylandSource::new(conn.clone(), event_queue)
			.insert(loop_handle)
			.context("Unable to create wayland source")?;

		Ok(Self {
			conn,
			globals,
			qh,
			event_loop,
			_token: token,
		})
	}

	/// Dispatches the event loop.
	///
	/// Consumes all existing events, and returns after.
	/// Does not wait for any events.
	pub fn dispatch(&mut self, state: &mut WaylandState<A>) -> Result<(), AppError> {
		self.event_loop
			.dispatch(Some(Duration::ZERO), state)
			.context("Unable to dispatch event loop")?;

		Ok(())
	}
}

impl<A> WaylandEventLoop<A> {
	/// Gets the wayland connection
	#[must_use]
	pub fn conn(&self) -> &Connection {
		&self.conn
	}
}
