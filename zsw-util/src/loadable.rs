//! Loadable

// Imports
use {
	crate::AppError,
	core::{fmt, task::Poll},
	futures::FutureExt,
};

/// Loadable value
pub struct Loadable<T, F> {
	/// Current value, if any
	value: Option<T>,

	/// Loading task
	task: Option<tokio::task::JoinHandle<T>>,

	/// Loader
	loader: F,
}

impl<T, F> Loadable<T, F> {
	/// Creates a new, empty, loadable
	#[must_use]
	pub fn new(loader: F) -> Self {
		Self {
			value: None,
			task: None,
			loader,
		}
	}

	/// Gets the inner value, if any.
	///
	/// Does not start loading if unloaded.
	pub fn get(&self) -> Option<&T> {
		self.value.as_ref()
	}

	/// Takes the value, if any.
	///
	/// Does not start loading if unloaded.
	pub fn take(&mut self) -> Option<T> {
		self.value.take()
	}

	/// Tries to load the inner value
	pub fn try_load(&mut self) -> Option<&mut T>
	where
		T: Send + 'static,
		F: Loader<T>,
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
				let Poll::Ready(res) = task.poll_unpin(&mut std::task::Context::from_waker(std::task::Waker::noop()))
				else {
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
				let fut = (self.loader)();
				self.task = Some(tokio::spawn(fut));

				None
			},
		}
	}
}

/// Loader trait
pub trait Loader<T> = AsyncFnMut() -> T where for<'a> <Self as AsyncFnMut<()>>::CallRefFuture<'a>: Send + 'static;

impl<T: fmt::Debug, F> fmt::Debug for Loadable<T, F> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Loadable")
			.field("value", &self.value)
			.field("task", &self.task)
			.field("loader", &"..")
			.finish()
	}
}
