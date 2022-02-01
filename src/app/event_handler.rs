//! Event handler

// Imports
use {
	crate::{Egui, Wgpu},
	crossbeam::atomic::AtomicCell,
	winit::{
		dpi::PhysicalPosition,
		event::{Event, WindowEvent},
		event_loop::ControlFlow as EventLoopControlFlow,
	},
};

/// Event handler
pub struct EventHandler<'a> {
	/// Wgpu
	wgpu: &'a Wgpu<'a>,

	/// Egui
	egui: &'a Egui,

	/// Cursor position
	cursor_pos: Option<PhysicalPosition<f64>>,

	/// Queued settings window open click
	queued_settings_window_open_click: &'a AtomicCell<Option<PhysicalPosition<f64>>>,
}

impl<'a> EventHandler<'a> {
	/// Creates the event handler
	pub fn new(
		wgpu: &'a Wgpu,
		egui: &'a Egui,
		queued_settings_window_open_click: &'a AtomicCell<Option<PhysicalPosition<f64>>>,
	) -> Self {
		Self {
			wgpu,
			egui,
			cursor_pos: None,
			queued_settings_window_open_click,
		}
	}

	/// Handles an event
	pub fn handle_event(&mut self, event: Event<!>, control_flow: &mut EventLoopControlFlow) {
		// Update egui
		self.egui.platform().lock().handle_event(&event);

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
				WindowEvent::Resized(size) => self.wgpu.resize(size),

				// On move, update the cursor position
				WindowEvent::CursorMoved { position, .. } => self.cursor_pos = Some(position),

				// If right clicked, queue a click
				WindowEvent::MouseInput {
					state: winit::event::ElementState::Pressed,
					button: winit::event::MouseButton::Right,
					..
				} => {
					self.queued_settings_window_open_click.store(self.cursor_pos);
				},
				_ => (),
			},
			_ => (),
		}
	}
}
