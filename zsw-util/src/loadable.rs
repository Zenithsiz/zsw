//! Loadable

use std::sync::oneshot;

/// Loadable value
#[derive(Debug)]
pub struct Loadable<T> {
	/// Current value, if any
	value: Option<T>,

	/// Receiver
	rx: Option<oneshot::Receiver<T>>,
}

impl<T> Loadable<T> {
	/// Creates a new, empty, loadable
	#[must_use]
	pub fn new() -> Self {
		Self {
			value: None,
			rx:    None,
		}
	}

	/// Gets the inner value, if any.
	///
	/// Does not start loading if unloaded.
	pub fn get(&self) -> Option<&T> {
		self.value.as_ref()
	}

	/// Gets the inner value mutably, if any.
	///
	/// Does not start loading if unloaded.
	pub fn get_mut(&mut self) -> Option<&mut T> {
		self.value.as_mut()
	}

	/// Takes the value, if any.
	///
	/// Does not start loading if unloaded.
	pub fn take(&mut self) -> Option<T> {
		self.value.take()
	}

	/// Tries to load the inner value.
	///
	/// If the value isn't loading, `spawn_task` is called to spawn a
	/// task that loads the value
	pub fn try_load<F>(&mut self, spawn_task: F) -> Option<&mut T>
	where
		T: Send + 'static,
		F: FnOnce(oneshot::Sender<T>),
	{
		// If the value is loaded, we're done
		// Note: We can't use if-let due to a borrow-checker limitation
		if self.value.is_some() {
			return self.value.as_mut();
		}

		// Otherwise, create or continue the playlist task
		match self.rx.take() {
			Some(rx) => match rx.try_recv() {
				Ok(value) => Some(self.value.insert(value)),
				Err(oneshot::TryRecvError::Empty(rx)) => {
					self.rx = Some(rx);
					None
				},
				Err(oneshot::TryRecvError::Disconnected) => {
					tracing::warn!("Task exited without returning a value");
					None
				},
			},
			None => {
				let (tx, rx) = oneshot::channel();
				self.rx = Some(rx);
				spawn_task(tx);

				None
			},
		}
	}
}

impl<T> Default for Loadable<T> {
	fn default() -> Self {
		Self::new()
	}
}
