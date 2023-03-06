//! Async mutex locking.
//!
//! Uses the `tokio::sync::Mutex` mutex.

// Imports
use {
	super::Locker,
	tokio::sync::{Mutex, MutexGuard},
};

/// Async mutex resource
#[sealed::sealed(pub(super))]
pub trait AsyncMutexResource {
	/// Inner type
	type Inner;

	/// Returns the inner mutex
	fn as_inner(&self) -> &Mutex<Self::Inner>;
}

/// Async mutex resource extension trait
#[extend::ext(name = AsyncMutexResourceExt)]
pub impl<R: AsyncMutexResource> R {
	/// Locks this mutex
	#[track_caller]
	async fn lock<'locker, const STATE: usize>(
		&'locker self,
		_locker: &'locker mut Locker<STATE>,
	) -> (
		MutexGuard<'locker, R::Inner>,
		Locker<{ <Locker<STATE> as AsyncMutexLocker<R>>::NEXT_STATE }>,
	)
	where
		Locker<STATE>: AsyncMutexLocker<R>,
		R::Inner: 'locker,
		[(); <Locker<STATE> as AsyncMutexLocker<R>>::NEXT_STATE]:,
	{
		let guard = self.as_inner().lock().await;
		(guard, Locker(()))
	}

	/// Locks this mutex blockingly
	#[track_caller]
	fn blocking_lock<'locker, const STATE: usize>(
		&'locker self,
		_locker: &'locker mut Locker<STATE>,
	) -> (
		MutexGuard<'locker, R::Inner>,
		Locker<{ <Locker<STATE> as AsyncMutexLocker<R>>::NEXT_STATE }>,
	)
	where
		Locker<STATE>: AsyncMutexLocker<R>,
		R::Inner: 'locker,
		[(); <Locker<STATE> as AsyncMutexLocker<R>>::NEXT_STATE]:,
	{
		let guard = self.as_inner().blocking_lock();
		(guard, Locker(()))
	}
}

/// Locker for async mutexes
#[sealed::sealed(pub(super))]
pub trait AsyncMutexLocker<R> {
	const NEXT_STATE: usize;
}

/// Creates a mutex resource type
pub macro resource_impl(
	$Name:ident($Inner:ty);
	fn $new:ident(...) -> ...;

	states {
		$( $CUR_STATE:literal => $NEXT_STATE:literal ),* $(,)?
	}
) {
	#[derive(Debug)]
	pub struct $Name(Mutex<$Inner>);

	impl $Name {
		/// Creates the mutex
		pub fn $new(inner: $Inner) -> Self {
			Self(Mutex::new(inner))
		}
	}

	#[sealed::sealed]
	impl AsyncMutexResource for $Name {
		type Inner = $Inner;

		fn as_inner(&self) -> &Mutex<Self::Inner> {
			&self.0
		}
	}

	$(
		#[sealed::sealed]
		impl AsyncMutexLocker<$Name> for Locker<$CUR_STATE> {
			const NEXT_STATE: usize = $NEXT_STATE;
		}
	)*
}
