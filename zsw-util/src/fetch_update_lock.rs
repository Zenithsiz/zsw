//! Fetch-update lock

// Imports
use {
	crate::{self as zsw_util, extse::AsyncLockMutexSe, MightBlock},
	futures::{
		lock::{Mutex, MutexGuard},
		Future,
	},
	std::{
		collections::VecDeque,
		mem,
		ops::{Deref, DerefMut},
		pin::Pin,
		task::{self, Waker},
	},
	zsw_side_effect_macros::side_effect,
};

/// Inner
#[derive(Debug)]
struct Inner<T> {
	/// Value
	value: T,

	/// If this value has been seen
	seen: bool,

	/// All wakers
	wakers: VecDeque<Waker>,
}

/// Fetch-update lock
///
/// This lock holds a value that may be updated and fetched
#[derive(Debug)]
pub struct FetchUpdateLock<T> {
	/// Inner
	inner: Mutex<Inner<T>>,
}

impl<T> FetchUpdateLock<T> {
	/// Creates a new fetch-update lock
	pub fn new(value: T) -> Self {
		let inner = Inner {
			value,
			seen: false,
			wakers: VecDeque::new(),
		};
		Self {
			inner: Mutex::new(inner),
		}
	}

	/// Fetches the value
	///
	/// # Blocking
	/// Waits until the lock is attained
	#[side_effect(MightBlock)]
	pub async fn fetch(&self) -> FetchUpdateLockGuard<'_, T>
	where
		T: Send,
	{
		// DEADLOCK: Caller ensures we can lock
		let mut inner = self.inner.lock_se().await.allow::<MightBlock>();

		// Set that the value was seen and wake up someone to update it
		inner.seen = true;
		if let Some(waker) = inner.wakers.pop_front() {
			waker.wake();
		}

		// Then return the value
		FetchUpdateLockGuard { guard: inner }
	}

	/// Updates the value.
	///
	/// If the previous value hasn't been seen yet, waits until
	/// it is seen.
	///
	/// # Blocking
	/// Waits until the lock is attained and the value is seen (with the lock unlocked).
	#[side_effect(MightBlock)]
	pub async fn update<F, U>(&self, f: F) -> U
	where
		T: Send,
		F: FnOnce(&mut T) -> U + Send,
	{
		// DEADLOCK: Caller ensures we can lock
		let mut inner = self.inner.lock_se().await.allow::<MightBlock>();

		// If the value is unseen, wait until it's seen
		// Note: Due to spurious wake ups, we loop until
		//       we're sure the value was seen
		while !inner.seen {
			// Wait until woken up
			// DEADLOCK: Caller ensures we can wait.
			//           We guarantee we unlock while waiting
			CondVarFuture::new(move |waker| {
				inner.wakers.push_back(waker.clone());
				mem::drop(inner);
			})
			.await;

			// Then get the lock gain
			// DEADLOCK: Caller ensures we can wait.
			inner = self.inner.lock_se().await.allow::<MightBlock>();
		}

		// Else update and set the value as not seen
		let output = f(&mut inner.value);
		inner.seen = false;
		output
	}
}

/// Guard
#[derive(Debug)]
pub struct FetchUpdateLockGuard<'a, T> {
	/// Guard
	guard: MutexGuard<'a, Inner<T>>,
}

impl<'a, T> FetchUpdateLockGuard<'a, T> {
	/// Sets this value as seen
	///
	/// This is done automatically on the creation of this
	/// lock, but if you called [`Self::set_unseen`], you may
	/// then set this value as seen again, so any updaters may
	/// update without waiting.
	pub fn set_seen(&mut self) {
		self.guard.seen = true;
	}

	/// Sets this value as not seen.
	///
	/// Any update waiting on this lock will continue
	/// to wait once this guard is dropped.
	pub fn set_unseen(&mut self) {
		self.guard.seen = false;
	}
}

impl<'a, T> Deref for FetchUpdateLockGuard<'a, T> {
	type Target = T;

	fn deref(&self) -> &Self::Target {
		&self.guard.value
	}
}

impl<'a, T> DerefMut for FetchUpdateLockGuard<'a, T> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.guard.value
	}
}

/// Condition variable future.
///
/// Calls `f` with a waker when polled the first time,
/// returning [`task::Poll::Pending`]. When polled a
/// second time, returns [`task::Poll::Ready`].
///
/// # Spurious wake ups
/// It is possible for spurious wake ups to occur,
/// if the runtime decides to poll a second time
/// without the waker being woken up.
///
/// You should create a new future and await again in this
/// case. Do *not* use the same future, as it will be exhausted
/// and return [`task::Poll::Ready`] always, entering a busy loop.
struct CondVarFuture<F> {
	/// Waker function
	f: Option<F>,
}

impl<F> CondVarFuture<F>
where
	F: FnOnce(&Waker),
{
	/// Creates a new future
	fn new(f: F) -> Self {
		Self { f: Some(f) }
	}
}

impl<F> Future for CondVarFuture<F>
where
	// TODO: Is it fine to require `Unpin` here?
	F: FnOnce(&Waker) + Unpin,
{
	type Output = ();

	fn poll(mut self: Pin<&mut Self>, ctx: &mut task::Context<'_>) -> task::Poll<Self::Output> {
		// Check if we still have the function
		match self.f.take() {
			// If we do, call it with the waker and return pending,

			// If we do, add ourselves to the wakers and return pending
			// Note: We also unlock the mutex on returning here
			Some(f) => {
				f(ctx.waker());
				task::Poll::Pending
			},

			// Else we've been woken up (possibly spuriously), so return ready
			None => task::Poll::Ready(()),
		}
	}
}
