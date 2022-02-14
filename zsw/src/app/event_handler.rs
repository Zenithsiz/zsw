//! Event handler

// Imports
use {
	winit::{
		dpi::PhysicalPosition,
		event::{Event, WindowEvent},
		event_loop::ControlFlow as EventLoopControlFlow,
	},
	zsw_egui::Egui,
	zsw_settings_window::SettingsWindow,
	zsw_side_effect_macros::side_effect,
	zsw_util::MightLock,
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
	///
	/// # Locking
	/// [`zsw_egui::PlatformLock`]
	#[side_effect(MightLock<zsw_egui::PlatformLock<'egui>>)]
	pub async fn handle_event<'window, 'egui>(
		&mut self,
		wgpu: &Wgpu<'_>,
		egui: &'egui Egui,
		settings_window: &SettingsWindow,
		event: Event<'window, !>,
		control_flow: &mut EventLoopControlFlow,
	) {
		// Update egui
		// DEADLOCK: Caller ensures we can call it
		{
			let mut platform_lock = egui.lock_platform().await.allow::<MightLock<zsw_egui::PlatformLock>>();
			egui.handle_event(&mut platform_lock, &event);
		}

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
