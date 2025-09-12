//! Loadable

// Imports
use {
	crate::AppError,
	core::{fmt, task::Poll},
	futures::FutureExt,
	std::task,
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

	/// Tries to load the inner value
	pub fn try_load<Args>(&mut self, args: Args) -> Option<&mut T>
	where
		T: Send + 'static,
		F: Loader<Args, T>,
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
				let name = self.loader.name(&args);
				let fut = self.loader.load(args);
				match tokio::task::Builder::new().name(&name).spawn(fut) {
					Ok(task) => self.task = Some(task),
					Err(err) => tracing::warn!("Unable to spawn task: {}", AppError::new(&err).pretty()),
				}

				None
			},
		}
	}
}

/// Loader trait
pub trait Loader<Args, T> {
	/// Returns the loader name for a set of arguments
	// TODO: Is this the correct abstraction? Should we just have `load` return
	//       both the name and future?
	fn name(&self, args: &Args) -> String;

	/// Creates the loader future
	fn load(&mut self, args: Args) -> impl Future<Output = T> + Send + 'static;
}

impl<T: fmt::Debug, F> fmt::Debug for Loadable<T, F> {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("Loadable")
			.field("value", &self.value)
			.field("task", &self.task)
			.field("loader", &"..")
			.finish()
	}
}
