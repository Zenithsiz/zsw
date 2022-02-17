//! Fetch-update lock

// Imports
use {
	crate::{self as zsw_util, extse::AsyncLockMutexSe, MightBlock},
	futures::{
		lock::{Mutex, MutexGuard},
		Future,
	},
	std::{
		ops::{Deref, DerefMut},
		pin::Pin,
		task::{self, Waker},
	},
	zsw_side_effect_macros::side_effect,
};

/// Inner
#[derive(Debug)]
#[doc(hidden)]
struct Inner<T> {
	/// Value
	value: T,

	/// If this value has been seen
	seen: bool,

	/// All wakers
	wakers: Vec<Waker>,
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
			wakers: vec![],
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

		// Set that the value was seen and wake up everyone waiting for it
		// Note: We wake everyone to ensure no one stays waiting forever
		inner.seen = true;
		for waker in inner.wakers.drain(..) {
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
	/// Waits until the lock is attained and the value is seen.
	#[side_effect(MightBlock)]
	pub async fn update<F, U>(&self, f: F) -> U
	where
		T: Send,
		F: FnOnce(&mut T) -> U + Send,
	{
		// DEADLOCK: Caller ensures we can lock
		let mut inner = self.inner.lock_se().await.allow::<MightBlock>();

		// If the value is unseen, wait until it's seen
		// DEADLOCK: Caller ensures we can lock
		while !inner.seen {
			WaitSeenFuture::new(inner).await;
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

/// Future for waiting until the value is seen
// TODO: Move to it's own module as a condvar
struct WaitSeenFuture<'a, T> {
	/// Mutex guard
	guard: Option<MutexGuard<'a, Inner<T>>>,
}

impl<'a, T> WaitSeenFuture<'a, T> {
	/// Creates a new future
	fn new(guard: MutexGuard<'a, Inner<T>>) -> Self {
		Self { guard: Some(guard) }
	}
}

impl<'a, T> Future for WaitSeenFuture<'a, T> {
	type Output = ();

	fn poll(mut self: Pin<&mut Self>, ctx: &mut task::Context<'_>) -> task::Poll<Self::Output> {
		// Check if we have the lock
		match self.guard.take() {
			// If we do, add ourselves to the wakers and return pending
			// Note: We also unlock the mutex on returning here
			Some(mut inner) => {
				inner.wakers.push(ctx.waker().clone());
				task::Poll::Pending
			},

			// Else we've been woken up, so we can return with the mutex locked
			None => task::Poll::Ready(()),
		}
	}
}
