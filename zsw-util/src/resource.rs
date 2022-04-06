//! Resources

// Imports
use futures::lock::MutexLockFuture;

/// Resources bundle
pub trait ResourcesBundle {
	/// Retrieves the resource `Resource`
	fn resource<Resource>(&self) -> MutexLockFuture<Resource>
	where
		Self: self::Resources<Resource>,
	{
		self.lock()
	}
}

/// Resources bundle that can lock `Resource`
pub trait Resources<Resource>: ResourcesBundle {
	/// Locks and retrieves `Resource`
	// TODO: Proper future?
	// TODO: Use a locker instead of this
	fn lock(&self) -> MutexLockFuture<Resource>;
}
