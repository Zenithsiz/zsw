//! Async mutex locking.
//!
//! Uses the `tokio::sync::Mutex` mutex.

// Imports
use {
	super::AsyncLocker,
	tokio::sync::{Mutex, MutexGuard},
};

/// Async mutex resource
#[sealed::sealed(pub(super))]
pub trait AsyncMutexResource {
	/// Inner type
	type Inner;

	/// Returns the inner mutex
	#[doc(hidden)]
	fn as_inner(&self) -> &Mutex<Self::Inner>;

	/// Locks this mutex
	#[track_caller]
	async fn lock<'locker, 'prev_locker, const STATE: usize>(
		&'locker self,
		locker: &'locker mut AsyncLocker<'prev_locker, STATE>,
	) -> (
		MutexGuard<'locker, Self::Inner>,
		AsyncLocker<'locker, { <AsyncLocker<'locker, STATE> as AsyncMutexLocker<Self>>::NEXT_STATE }>,
	)
	where
		Self: Sized,
		AsyncLocker<'prev_locker, STATE>: AsyncMutexLocker<Self>,
		Self::Inner: 'locker,
		[(); <AsyncLocker<'prev_locker, STATE> as AsyncMutexLocker<Self>>::NEXT_STATE]:,
	{
		locker.start_awaiting();
		let guard = self.as_inner().lock().await;
		(guard, locker.next())
	}
}

/// AsyncLocker for async mutexes
#[sealed::sealed(pub(super))]
pub trait AsyncMutexLocker<R> {
	const NEXT_STATE: usize;
}

/// Creates a mutex resource type
pub macro resource_impl(
	$Name:ident { $field:ident: $Inner:ty };
	fn $new:ident(...) -> ...;

	states {
		$( $CUR_STATE:literal => $NEXT_STATE:literal ),* $(,)?
	}
) {
	#[derive(Debug)]
	pub struct $Name {
		$field: Mutex<$Inner>
	}

	impl $Name {
		/// Creates the mutex
		pub fn $new(inner: $Inner) -> Self {
			Self { $field: Mutex::new(inner) }
		}
	}

	#[sealed::sealed]
	impl AsyncMutexResource for $Name {
		type Inner = $Inner;

		fn as_inner(&self) -> &Mutex<Self::Inner> {
			&self.$field
		}
	}

	$(
		#[sealed::sealed]
		impl AsyncMutexLocker<$Name> for AsyncLocker<'_, $CUR_STATE> {
			const NEXT_STATE: usize = $NEXT_STATE;
		}
	)*
}
