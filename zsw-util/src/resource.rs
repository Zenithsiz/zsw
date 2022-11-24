//! Resources

// Imports
use std::{
	future::Future,
	ops::{Deref, DerefMut},
};

/// Resources bundle
pub trait ResourcesBundle {
	/// Retrieves the resource `R`
	async fn resource<R>(&mut self) -> <Self as Resources<R>>::Resource<'_>
	where
		Self: Resources<R>,
	{
		self.lock().await
	}

	/// Retrieves the resources 2-tuple `(T1, T2)`
	async fn resources_tuple2<T1, T2>(
		&mut self,
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
	type Resource<'a>: Deref<Target = R> + DerefMut
	where
		Self: 'a;

	/// Future type for [`Self::lock`]
	type LockFuture<'a>: Future<Output = Self::Resource<'a>>
	where
		Self: 'a;

	/// Locks and retrieves `Resource`
	fn lock(&mut self) -> Self::LockFuture<'_>;
}

/// Resources 2-tuple
pub trait ResourcesTuple2<T1, T2>: ResourcesBundle {
	// Resources
	type Resources1<'a>: Deref<Target = T1> + DerefMut
	where
		Self: 'a;
	type Resources2<'a>: Deref<Target = T2> + DerefMut
	where
		Self: 'a;

	/// Future type for [`Self::lock`]
	type LockFuture<'a>: Future<Output = (Self::Resources1<'a>, Self::Resources2<'a>)>
	where
		Self: 'a;

	/// Locks and retrieves `Resources`
	fn lock(&mut self) -> Self::LockFuture<'_>;
}
