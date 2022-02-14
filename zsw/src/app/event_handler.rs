//! Event handler

// Imports
use {
	super::settings_window::SettingsWindow,
	winit::{
		dpi::PhysicalPosition,
		event::{Event, WindowEvent},
		event_loop::ControlFlow as EventLoopControlFlow,
	},
	zsw_egui::Egui,
	zsw_wgpu::Wgpu,
};

/// Event handler
pub struct EventHandler {
	/// Cursor position
	cursor_pos: Option<PhysicalPosition<f64>>,
}

impl EventHandler {
	/// Creates the event handler
	pub fn new() -> Self {
		Self { cursor_pos: None }
	}

	/// Handles an event
	pub fn handle_event(
		&mut self,
		wgpu: &Wgpu,
		egui: &Egui,
		settings_window: &SettingsWindow,
		event: Event<!>,
		control_flow: &mut EventLoopControlFlow,
	) {
		// Update egui
		egui.handle_event(&event);

		// Set control for to wait for next event, since we're not doing
		// anything else on the main thread
		*control_flow = EventLoopControlFlow::Wait;

		// Then handle the event
		#[allow(clippy::single_match)] // We might add more in the future
		match event {
			Event::WindowEvent { event, .. } => match event {
				// If we should be closing, set the control flow to exit
				WindowEvent::CloseRequested | WindowEvent::Destroyed => {
					log::warn!("Received close request, closing window");
					*control_flow = EventLoopControlFlow::Exit;
				},

				// If we resized, queue a resize on wgpu
				WindowEvent::Resized(size) => wgpu.resize(size),

				// On move, update the cursor position
				WindowEvent::CursorMoved { position, .. } => self.cursor_pos = Some(position),

				// If right clicked, queue a click
				WindowEvent::MouseInput {
					state: winit::event::ElementState::Pressed,
					button: winit::event::MouseButton::Right,
					..
				} => settings_window.queue_open_click(self.cursor_pos),
				_ => (),
			},
			_ => (),
		}
	}
}
