//! Locker

// Imports
use {
	futures::lock::{Mutex, MutexGuard},
	std::{
		ops::{Deref, DerefMut},
		sync::Arc,
	},
	zsw_util::where_assert,
};

// Locks
type T0 = Option<crate::panel::PanelGroup>; // TODO: Use proper type for this instead of `Option<_>`
type T1 = crate::panel::PanelsRendererShader;
type T2 = ((),);
type T3 = (((),),);

/// All locks
#[derive(Debug)]
pub struct Locks(Mutex<T0>, Mutex<T1>, Mutex<T2>, Mutex<T3>);

impl Locks {
	/// Creates all the locks
	pub fn new(t0: T0, t1: T1, t2: T2, t3: T3) -> Self {
		Self(Mutex::new(t0), Mutex::new(t1), Mutex::new(t2), Mutex::new(t3))
	}
}

/// Locker
#[derive(Debug)]
pub struct Locker<const STATE: usize = 0> {
	/// All locks
	locks: Arc<Locks>,
}

impl<const STATE: usize> Locker<STATE> {
	/// Creates a new locker
	pub fn new(locks: Locks) -> Self {
		Self { locks: Arc::new(locks) }
	}

	/// Clones this locker.
	///
	/// # Thread safety
	/// Should not be called to create two lockers per task
	pub fn clone_unchecked(&self) -> Self {
		Self {
			locks: Arc::clone(&self.locks),
		}
	}

	/// Downgrades this locker to a next locking state
	///
	/// # Thread safety
	/// Should not be called to create two lockers per task
	fn downgrade<const NEXT_STATE: usize>(&self) -> Locker<NEXT_STATE>
	where
		where_assert!(NEXT_STATE > STATE):,
	{
		Locker {
			locks: Arc::clone(&self.locks),
		}
	}

	/// Locks a resource
	#[track_caller]
	pub async fn resource<'locker, R>(
		&'locker mut self,
	) -> (Resource<'locker, R>, <Self as Lockable<R>>::NextLocker<'locker>)
	where
		Self: Lockable<R>,
		R: 'locker,
	{
		self.lock_resource().await
	}
}

// TODO: Replace with const generics `NEXT_STATE > CUR_STATE` impl
#[duplicate::duplicate_item(
	ResourceTy field CurState NextState;
	duplicate! {
		[ CurState; [0] ]
		[T0] [0] [ CurState ] [ 1 ];
	}
	duplicate! {
		[ CurState; [0]; [1] ]
		[T1] [1] [ CurState ] [ 2 ];
	}
	duplicate! {
		[ CurState; [0]; [1]; [2] ]
		[T2] [2] [ CurState ] [ 3 ];
	}
	duplicate! {
		[ CurState; [0]; [1]; [2]; [3] ]
		[T3] [3] [ CurState ] [ 4 ];
	}
)]
impl Lockable<ResourceTy> for Locker<CurState> {
	type NextLocker<'locker> = Locker<NextState>
	where
		Self: 'locker;

	#[track_caller]
	async fn lock_resource<'locker>(&'locker mut self) -> (Resource<ResourceTy>, Self::NextLocker<'locker>)
	where
		ResourceTy: 'locker,
	{
		#[cfg(feature = "locker-trace")]
		tracing::trace!(resource = ?std::any::type_name::<ResourceTy>(), backtrace = %std::backtrace::Backtrace::force_capture(), "Locking resource");
		let guard = self.locks.field.lock().await;

		#[cfg(feature = "locker-trace")]
		tracing::trace!(resource = ?std::any::type_name::<ResourceTy>(), backtrace = %std::backtrace::Backtrace::force_capture(), "Locked resource");
		let resource = Resource { guard };

		(resource, self.downgrade())
	}
}

/// A lock-able type
#[doc(hidden)] // Implementation detail
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
