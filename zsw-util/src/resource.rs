//! Resources

// Imports
use std::ops::{Deref, DerefMut};

/// Resources bundle
pub trait ResourcesBundle {
	/// Retrieves the resource `R`
	async fn resource<R>(&self) -> <Self as Resources<R>>::Resource<'_>
	where
		Self: Resources<R>,
	{
		self.lock().await
	}

	/// Retrieves the resources 2-tuple `(T1, T2)`
	async fn resources_tuple2<T1, T2>(
		&self,
	) -> (
		<Self as ResourcesTuple2<T1, T2>>::Resources1<'_>,
		<Self as ResourcesTuple2<T1, T2>>::Resources2<'_>,
	)
	where
		Self: ResourcesTuple2<T1, T2>,
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

/// Resources 2-tuple
pub trait ResourcesTuple2<T1, T2>: ResourcesBundle {
	// Resources
	type Resources1<'a>: Deref<Target = T1> + DerefMut;
	type Resources2<'a>: Deref<Target = T2> + DerefMut;

	/// Locks and retrieves `Resources`
	async fn lock(&self) -> (Self::Resources1<'_>, Self::Resources2<'_>);
}
