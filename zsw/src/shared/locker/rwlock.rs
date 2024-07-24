//! Async rwlock locking.
//!
//! Uses the `tokio::sync::RwLock` rwlock.

// Imports
use {
	super::AsyncLocker,
	tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard},
};

/// Async rwlock resource
#[sealed::sealed(pub(super))]
pub trait AsyncRwLockResource {
	/// Inner type
	type Inner;

	/// Returns the inner rwlock
	#[doc(hidden)]
	fn as_inner(&self) -> &RwLock<Self::Inner>;

	/// Locks this rwlock for reads
	async fn read<'locker, 'task, const STATE: usize>(
		&'locker self,
		locker: &'locker mut AsyncLocker<'task, STATE>,
	) -> (
		RwLockReadGuard<'locker, Self::Inner>,
		AsyncLocker<'locker, { <AsyncLocker<'task, STATE> as AsyncRwLockLocker<Self>>::NEXT_STATE }>,
	)
	where
		Self: Sized,
		AsyncLocker<'task, STATE>: AsyncRwLockLocker<Self>,
		Self::Inner: 'locker,
		[(); <AsyncLocker<'task, STATE> as AsyncRwLockLocker<Self>>::NEXT_STATE]:,
	{
		locker.start_awaiting();
		let guard = self.as_inner().read().await;
		(guard, locker.next())
	}

	/// Locks this rwlock for writes
	async fn write<'locker, 'task, const STATE: usize>(
		&'locker self,
		locker: &'locker mut AsyncLocker<'task, STATE>,
	) -> (
		RwLockWriteGuard<'locker, Self::Inner>,
		AsyncLocker<'locker, { <AsyncLocker<'task, STATE> as AsyncRwLockLocker<Self>>::NEXT_STATE }>,
	)
	where
		Self: Sized,
		AsyncLocker<'task, STATE>: AsyncRwLockLocker<Self>,
		Self::Inner: 'locker,
		[(); <AsyncLocker<'task, STATE> as AsyncRwLockLocker<Self>>::NEXT_STATE]:,
	{
		locker.start_awaiting();
		let guard = self.as_inner().write().await;
		(guard, locker.next())
	}
}

/// AsyncLocker for async rwlocks
#[sealed::sealed(pub(super))]
pub trait AsyncRwLockLocker<R> {
	const NEXT_STATE: usize;
}

/// Creates a rwlock resource type
pub macro resource_impl(
	$Name:ident { $field:ident: $Inner:ty };
	fn $new:ident(...) -> ...;

	states {
		$( $CUR_STATE:literal => $NEXT_STATE:literal ),* $(,)?
	}
) {
	#[derive(Debug)]
	pub struct $Name {
		$field: RwLock<$Inner>
	}

	impl $Name {
		/// Creates the rwlock
		pub fn $new(inner: $Inner) -> Self {
			Self { $field: RwLock::new(inner) }
		}
	}

	#[sealed::sealed]
	impl AsyncRwLockResource for $Name {
		type Inner = $Inner;

		fn as_inner(&self) -> &RwLock<Self::Inner> {
			&self.$field
		}
	}

	$(
		#[sealed::sealed]
		impl AsyncRwLockLocker<$Name> for AsyncLocker<'_, $CUR_STATE> {
			const NEXT_STATE: usize = $NEXT_STATE;
		}
	)*
}
