//! Loadable

// Imports
use {crate::AppError, core::task::Poll, futures::FutureExt, std::task};

/// Loadable value
#[derive(Debug)]
pub struct Loadable<T> {
	/// Current value, if any
	value: Option<T>,

	/// Loading task
	task: Option<tokio::task::JoinHandle<T>>,
}

impl<T> Loadable<T> {
	/// Creates a new, empty, loadable
	#[must_use]
	pub fn new() -> Self {
		Self {
			value: None,
			task:  None,
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
		F: FnOnce() -> Result<tokio::task::JoinHandle<T>, AppError>,
	{
		// If the value is loaded, we're done
		// Note: We can't use if-let due to a borrow-checker limitation
		if self.value.is_some() {
			return self.value.as_mut();
		}

		// Otherwise, create or continue the playlist task
		match &mut self.task {
			Some(task) => {
				// Otherwise, try to get the value
				let mut cx = task::Context::from_waker(task::Waker::noop());
				let Poll::Ready(res) = task.poll_unpin(&mut cx) else {
					return None;
				};
				self.task = None;

				match res {
					Ok(value) => {
						let value = self.value.insert(value);
						Some(value)
					},
					Err(err) => {
						let err = AppError::new(&err);
						tracing::warn!("Task returned an unexpected error: {}", err.pretty());

						None
					},
				}
			},
			None => {
				match spawn_task() {
					Ok(task) => self.task = Some(task),
					Err(err) => tracing::warn!("Unable to spawn task: {}", err.pretty()),
				}

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
