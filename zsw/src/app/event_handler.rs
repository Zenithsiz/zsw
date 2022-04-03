//! Event handler

// Imports
use {
	winit::{
		event::{DeviceEvent, Event, WindowEvent},
		event_loop::ControlFlow as EventLoopControlFlow,
	},
	zsw_egui::Egui,
	zsw_input::Input,
	zsw_settings_window::SettingsWindow,
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
	///
	/// # Blocking
	/// Locks [`zsw_egui::PlatformLock`] on `egui`
	// TODO: Inverse dependencies of `settings_window` and `panels` and let them depend on us
	pub async fn handle_event<'window, 'egui>(
		&mut self,
		wgpu: &Wgpu<'_>,
		egui: &'egui Egui,
		settings_window: &SettingsWindow,
		input: &Input,
		event: Event<'window, !>,
		control_flow: &mut EventLoopControlFlow,
	) {
		// Update egui
		// DEADLOCK: Caller ensures we can call it
		{
			let mut platform_lock = egui.lock_platform().await;
			egui.handle_event(&mut platform_lock, &event);
		}

		// Set control for to wait for next event, since we're not doing
		// anything else on the main thread
		*control_flow = EventLoopControlFlow::Wait;

		// Then handle the event
		#[allow(clippy::collapsible_match)] // We might add more in the future
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
				WindowEvent::CursorMoved { position, .. } => {
					input.update_cursor_pos(position);
				},

				// If right clicked, queue a click
				WindowEvent::MouseInput {
					state: winit::event::ElementState::Pressed,
					button: winit::event::MouseButton::Right,
					..
				} => settings_window.queue_open_click(input.cursor_pos()),
				_ => (),
			},

			#[allow(clippy::single_match)] // We might add more in the future
			Event::DeviceEvent { event, .. } => match event {
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
	}
}
