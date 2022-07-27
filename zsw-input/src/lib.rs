//! Input

// Imports
use {crossbeam::atomic::AtomicCell, winit::dpi::PhysicalPosition};

/// Input
#[derive(Debug)]
pub struct Input {
	/// Current cursor position
	cursor_pos: AtomicCell<Option<PhysicalPosition<f64>>>,
}

impl Input {
	/// Creates a new input
	#[must_use]
	pub fn new() -> Self {
		Self {
			cursor_pos: AtomicCell::new(None),
		}
	}

	/// Updates the cursor position
	pub fn update_cursor_pos(&self, pos: PhysicalPosition<f64>) {
		self.cursor_pos.store(Some(pos));
	}

	/// Returns the cursor position
	pub fn cursor_pos(&self) -> Option<PhysicalPosition<f64>> {
		self.cursor_pos.load()
	}
}

impl Default for Input {
	fn default() -> Self {
		Self::new()
	}
}
