//! Async rwlock locking.
//!
//! Uses the `tokio::sync::RwLock` rwlock.

// Imports
use {
	super::Locker,
	tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard},
};

/// Async rwlock resource
#[sealed::sealed(pub(super))]
pub trait AsyncRwLockResource {
	/// Inner type
	type Inner;

	/// Returns the inner rwlock
	fn as_inner(&self) -> &RwLock<Self::Inner>;
}

/// Async rwlock resource extension trait
#[extend::ext(name = AsyncRwLockResourceExt)]
#[sealed::sealed]
pub impl<R: AsyncRwLockResource> R {
	/// Locks this rwlock for reads
	#[track_caller]
	async fn read<'locker, const STATE: usize>(
		&'locker self,
		_locker: &'locker mut Locker<STATE>,
	) -> (
		RwLockReadGuard<'locker, R::Inner>,
		Locker<{ <Locker<STATE> as AsyncRwLockLocker<R>>::NEXT_STATE }>,
	)
	where
		Locker<STATE>: AsyncRwLockLocker<R>,
		R::Inner: 'locker,
		[(); <Locker<STATE> as AsyncRwLockLocker<R>>::NEXT_STATE]:,
	{
		let guard = self.as_inner().read().await;
		(guard, Locker(()))
	}

	/// Locks this rwlock for reads blockingly
	#[track_caller]
	fn blocking_read<'locker, const STATE: usize>(
		&'locker self,
		_locker: &'locker mut Locker<STATE>,
	) -> (
		RwLockReadGuard<'locker, R::Inner>,
		Locker<{ <Locker<STATE> as AsyncRwLockLocker<R>>::NEXT_STATE }>,
	)
	where
		Locker<STATE>: AsyncRwLockLocker<R>,
		R::Inner: 'locker,
		[(); <Locker<STATE> as AsyncRwLockLocker<R>>::NEXT_STATE]:,
	{
		let guard = self.as_inner().blocking_read();
		(guard, Locker(()))
	}

	/// Locks this rwlock for writes
	#[track_caller]
	async fn write<'locker, const STATE: usize>(
		&'locker self,
		_locker: &'locker mut Locker<STATE>,
	) -> (
		RwLockWriteGuard<'locker, R::Inner>,
		Locker<{ <Locker<STATE> as AsyncRwLockLocker<R>>::NEXT_STATE }>,
	)
	where
		Locker<STATE>: AsyncRwLockLocker<R>,
		R::Inner: 'locker,
		[(); <Locker<STATE> as AsyncRwLockLocker<R>>::NEXT_STATE]:,
	{
		let guard = self.as_inner().write().await;
		(guard, Locker(()))
	}

	/// Locks this rwlock for writes blockingly
	#[track_caller]
	fn blocking_write<'locker, const STATE: usize>(
		&'locker self,
		_locker: &'locker mut Locker<STATE>,
	) -> (
		RwLockWriteGuard<'locker, R::Inner>,
		Locker<{ <Locker<STATE> as AsyncRwLockLocker<R>>::NEXT_STATE }>,
	)
	where
		Locker<STATE>: AsyncRwLockLocker<R>,
		R::Inner: 'locker,
		[(); <Locker<STATE> as AsyncRwLockLocker<R>>::NEXT_STATE]:,
	{
		let guard = self.as_inner().blocking_write();
		(guard, Locker(()))
	}
}

/// Locker for async rwlocks
#[sealed::sealed(pub(super))]
pub trait AsyncRwLockLocker<R> {
	const NEXT_STATE: usize;
}

/// Creates a rwlock resource type
pub macro resource_impl(
	$Name:ident($Inner:ty);
	fn $new:ident(...) -> ...;

	states {
		$( $CUR_STATE:literal => $NEXT_STATE:literal ),* $(,)?
	}
) {
	#[derive(Debug)]
	pub struct $Name(RwLock<$Inner>);

	impl $Name {
		/// Creates the rwlock
		pub fn $new(inner: $Inner) -> Self {
			Self(RwLock::new(inner))
		}
	}

	#[sealed::sealed]
	impl AsyncRwLockResource for $Name {
		type Inner = $Inner;

		fn as_inner(&self) -> &RwLock<Self::Inner> {
			&self.0
		}
	}

	$(
		#[sealed::sealed]
		impl AsyncRwLockLocker<$Name> for Locker<$CUR_STATE> {
			const NEXT_STATE: usize = $NEXT_STATE;
		}
	)*
}
