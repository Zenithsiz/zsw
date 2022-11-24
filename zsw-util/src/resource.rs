//! Resources

// Imports
use std::ops::{Deref, DerefMut};

/// Resources bundle
pub trait ResourcesBundle {
	/// Retrieves the resource `Resource`
	async fn resource<R>(&self) -> <Self as Resources<R>>::Resource<'_>
	where
		Self: Resources<R>,
	{
		self.lock().await
	}
}

/// Resources bundle that can lock `Resource`
pub trait Resources<R>: ResourcesBundle {
	/// Resource wrapper
	type Resource<'a>: Deref<Target = R> + DerefMut;

	/// Locks and retrieves `Resource`
	async fn lock(&self) -> Self::Resource<'_>;
}
