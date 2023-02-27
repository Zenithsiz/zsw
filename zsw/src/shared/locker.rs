//! Locker

// Imports
use {
	futures::lock::MutexGuard,
	std::ops::{Deref, DerefMut},
};

/// A lock-able type
pub trait Lockable<R> {
	/// Next locker
	type NextLocker<'locker>
	where
		Self: 'locker;

	/// Locks the resource `R` and returns the next locker
	async fn lock_resource<'locker>(&'locker mut self) -> (Resource<R>, Self::NextLocker<'locker>)
	where
		R: 'locker;
}

/// Resource
#[derive(Debug)]
pub struct Resource<'locker, R> {
	/// Lock guard
	guard: MutexGuard<'locker, R>,
}

impl<'locker, R> Resource<'locker, R> {
	/// Creates a resource from the mutex guard for the resource
	pub fn new(guard: MutexGuard<'locker, R>) -> Self {
		Self { guard }
	}
}

#[cfg(feature = "locker-trace")]
impl<'locker, R> Drop for Resource<'locker, R> {
	#[track_caller]
	fn drop(&mut self) {
		tracing::trace!(resource = ?std::any::type_name::<R>(), backtrace = %std::backtrace::Backtrace::force_capture(), "Dropping resource");
	}
}

impl<'locker, R> Deref for Resource<'locker, R> {
	type Target = R;

	fn deref(&self) -> &Self::Target {
		&self.guard
	}
}
impl<'locker, R> DerefMut for Resource<'locker, R> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.guard
	}
}
