//! Event handler

// Imports
use {
	winit::{
		event::{DeviceEvent, Event, WindowEvent},
		event_loop::ControlFlow as EventLoopControlFlow,
	},
	zsw_egui::EguiEventHandler,
	zsw_input::InputUpdater,
};

/// Event handler
pub struct EventHandler {}

impl EventHandler {
	/// Creates the event handler
	pub fn new() -> Self {
		Self {}
	}

	/// Handles an event
	#[allow(clippy::unused_self)] // We might use it in the future
	pub fn handle_event(
		&mut self,
		event: Event<'_, !>,
		control_flow: &mut EventLoopControlFlow,
		egui_event_handler: &mut EguiEventHandler,
		input_updater: &mut InputUpdater,
	) {
		// Handle the event
		let event_status = self::handle_event(&event, input_updater);

		// Then update egui, if we should
		if event_status.update_egui && let Some(event) = event.to_static() {
			egui_event_handler.handle_event(event);
		}

		// Then set the control flow
		*control_flow = event_status.control_flow;
	}
}

/// Status after an event
struct EventStatus {
	/// Control flow
	control_flow: EventLoopControlFlow,

	/// If egui should be updated with this event
	update_egui: bool,
}

/// Handles an event
fn handle_event(event: &Event<'_, !>, input_updater: &mut InputUpdater) -> EventStatus {
	// Default event status
	let mut event_status = EventStatus {
		control_flow: EventLoopControlFlow::Wait,
		update_egui:  true,
	};

	#[allow(clippy::collapsible_match)] // We might add more in the future
	match *event {
		Event::WindowEvent { ref event, .. } => match *event {
			// If we should be closing, set the control flow to exit
			// Note: No point in updating egui if we're exiting
			WindowEvent::CloseRequested | WindowEvent::Destroyed => {
				tracing::warn!("Received close request, closing window");
				event_status.control_flow = EventLoopControlFlow::Exit;
				event_status.update_egui = false;
			},

			// If we resized, queue a resize on wgpu
			WindowEvent::Resized(size) => input_updater.on_resize(size),

			// On move, update the cursor position
			WindowEvent::CursorMoved { position, .. } => input_updater.update_cursor_pos(position),

			// If right clicked, send an on-click
			WindowEvent::MouseInput {
				state: winit::event::ElementState::Pressed,
				button,
				..
			} => input_updater.on_click(button),

			_ => (),
		},

		#[allow(clippy::single_match)] // We might add more in the future
		Event::DeviceEvent { ref event, .. } => match *event {
			// Note: We use mouse motion to keep track of the mouse while it's not
			//       on the desktop window
			DeviceEvent::MouseMotion {
				delta: (delta_x, delta_y),
			} =>
				if let Some(mut cursor_pos) = input_updater.cursor_pos() {
					cursor_pos.x += delta_x;
					cursor_pos.y += delta_y;
					input_updater.update_cursor_pos(cursor_pos);
				},
			_ => (),
		},

		_ => (),
	}

	event_status
}
