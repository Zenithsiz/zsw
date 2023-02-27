//! Locker

// Imports
use futures::lock::MutexGuard;

/// Locker of async mutex `R`
pub trait AsyncMutexLocker<R> {
	/// Next locker
	type Next<'locker>
	where
		Self: 'locker;

	/// Locks the resource `R` and returns the next locker
	async fn lock_resource<'locker>(&'locker mut self) -> (MutexGuard<R>, Self::Next<'locker>)
	where
		R: 'locker;
}
