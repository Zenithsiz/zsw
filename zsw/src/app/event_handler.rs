//! Event handler

// Imports
use {
	winit::{
		event::{DeviceEvent, Event, WindowEvent},
		event_loop::ControlFlow as EventLoopControlFlow,
	},
	zsw_egui::EguiEventHandler,
	zsw_input::Input,
	zsw_settings_window::SettingsWindow,
	zsw_util::Services,
	zsw_wgpu::Wgpu,
};

/// Event handler
pub struct EventHandler {}

impl EventHandler {
	/// Creates the event handler
	pub fn new() -> Self {
		Self {}
	}

	/// Handles an event
	// TODO: Invert dependencies of `settings_window` and `panels` and let them depend on us
	pub async fn handle_event<S>(
		&mut self,
		services: &S,
		event: Event<'_, !>,
		control_flow: &mut EventLoopControlFlow,
		egui_event_handler: &mut EguiEventHandler,
	) where
		S: Services<Wgpu> + Services<SettingsWindow> + Services<Input>,
	{
		let wgpu = services.service::<Wgpu>();
		let settings_window = services.service::<SettingsWindow>();
		let input = services.service::<Input>();

		// Handle the event
		let event_status = self::handle_event(&event, wgpu, input, settings_window).await;

		// Then update egui, if we should
		if event_status.update_egui && let Some(event) = event.to_static() {
			egui_event_handler.handle_event(event).await;
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
async fn handle_event(
	event: &Event<'_, !>,
	wgpu: &Wgpu,
	input: &Input,
	settings_window: &SettingsWindow,
) -> EventStatus {
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
			WindowEvent::Resized(size) => wgpu.resize(size),

			// On move, update the cursor position
			WindowEvent::CursorMoved { position, .. } => input.update_cursor_pos(position),

			// If right clicked, queue a click
			// TODO: Don't queue the open click here? Feels kinda hacky
			WindowEvent::MouseInput {
				state: winit::event::ElementState::Pressed,
				button: winit::event::MouseButton::Right,
				..
			} => settings_window.queue_open_click(input.cursor_pos()).await,
			_ => (),
		},

		#[allow(clippy::single_match)] // We might add more in the future
		Event::DeviceEvent { ref event, .. } => match *event {
			// Note: We use mouse motion to keep track of the mouse while it's not
			//       on the desktop window
			DeviceEvent::MouseMotion {
				delta: (delta_x, delta_y),
			} =>
				if let Some(mut cursor_pos) = input.cursor_pos() {
					cursor_pos.x += delta_x;
					cursor_pos.y += delta_y;
					input.update_cursor_pos(cursor_pos);
				},
			_ => (),
		},

		_ => (),
	}

	event_status
}
