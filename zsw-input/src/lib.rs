//! Input

// Imports
use {
	crossbeam::atomic::AtomicCell,
	std::sync::Arc,
	tokio::sync::broadcast,
	winit::{dpi::PhysicalPosition, event::MouseButton},
};

/// Input inner
#[derive(Debug)]
pub struct InputInner {
	/// Current cursor position
	cursor_pos: AtomicCell<Option<PhysicalPosition<f64>>>,
}

/// Input updater
#[derive(Clone, Debug)]
pub struct InputUpdater {
	/// Inner
	inner: Arc<InputInner>,

	/// On click sender
	on_click_tx: broadcast::Sender<MouseButton>,
}

impl InputUpdater {
	/// Updates the cursor position
	pub fn update_cursor_pos(&self, pos: PhysicalPosition<f64>) {
		self.inner.cursor_pos.store(Some(pos));
	}

	/// Returns the cursor position
	#[must_use]
	pub fn cursor_pos(&self) -> Option<PhysicalPosition<f64>> {
		self.inner.cursor_pos.load()
	}

	/// Sends an on-click event
	pub fn on_click(&self, button: MouseButton) {
		let _ = self.on_click_tx.send(button);
	}
}

/// Input receiver
#[derive(Debug)]
pub struct InputReceiver {
	/// Inner
	inner: Arc<InputInner>,

	/// On click receiver
	on_click_rx: broadcast::Receiver<MouseButton>,
}

impl Clone for InputReceiver {
	fn clone(&self) -> Self {
		Self {
			inner:       self.inner.clone(),
			on_click_rx: self.on_click_rx.resubscribe(),
		}
	}
}

impl InputReceiver {
	/// Returns the cursor position
	#[must_use]
	pub fn cursor_pos(&self) -> Option<PhysicalPosition<f64>> {
		self.inner.cursor_pos.load()
	}

	/// Returns any on-click events, if any
	pub fn on_click(&mut self) -> Option<MouseButton> {
		self.on_click_rx.try_recv().ok()
	}
}

/// Creates the input service
#[must_use]
pub fn create() -> (InputUpdater, InputReceiver) {
	let inner = Arc::new(InputInner {
		cursor_pos: AtomicCell::new(None),
	});

	let (on_click_tx, on_click_rx) = broadcast::channel(16);
	(
		InputUpdater {
			inner: Arc::clone(&inner),
			on_click_tx,
		},
		InputReceiver { inner, on_click_rx },
	)
}
