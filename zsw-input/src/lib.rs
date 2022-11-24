//! Input

// Imports
use {
	crossbeam::atomic::AtomicCell,
	std::sync::Arc,
	tokio::sync::broadcast,
	winit::{
		dpi::{PhysicalPosition, PhysicalSize},
		event::MouseButton,
	},
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

	/// On resize sender
	on_resize_tx: broadcast::Sender<PhysicalSize<u32>>,
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

	/// Sends an on-resize event
	pub fn on_resize(&self, size: PhysicalSize<u32>) {
		let _ = self.on_resize_tx.send(size);
	}
}

/// Input receiver
#[derive(Debug)]
pub struct InputReceiver {
	/// Inner
	inner: Arc<InputInner>,

	/// On click receiver
	on_click_rx: broadcast::Receiver<MouseButton>,

	/// On resize receiver
	on_resize_rx: broadcast::Receiver<PhysicalSize<u32>>,
}

impl Clone for InputReceiver {
	fn clone(&self) -> Self {
		Self {
			inner:        self.inner.clone(),
			on_click_rx:  self.on_click_rx.resubscribe(),
			on_resize_rx: self.on_resize_rx.resubscribe(),
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

	/// Returns any last on-resize event, if any
	pub fn on_resize(&mut self) -> Option<PhysicalSize<u32>> {
		self.on_resize_rx.try_recv().ok()
	}
}

/// Creates the input service
#[must_use]
pub fn create() -> (InputUpdater, InputReceiver) {
	let inner = Arc::new(InputInner {
		cursor_pos: AtomicCell::new(None),
	});

	let (on_click_tx, on_click_rx) = broadcast::channel(16);
	let (on_resize_tx, on_resize_rx) = broadcast::channel(16);
	(
		InputUpdater {
			inner: Arc::clone(&inner),
			on_click_tx,
			on_resize_tx,
		},
		InputReceiver {
			inner,
			on_click_rx,
			on_resize_rx,
		},
	)
}
