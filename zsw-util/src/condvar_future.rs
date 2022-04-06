//! Condvar future

// Imports
use {
	futures::Future,
	std::{
		pin::Pin,
		task::{self, Waker},
	},
};

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
#[derive(Debug)]
pub struct CondvarFuture<F> {
	/// Waker function
	f: Option<F>,
}

impl<F> CondvarFuture<F>
where
	F: FnOnce(&Waker),
{
	/// Creates a new future
	pub fn new(f: F) -> Self {
		Self { f: Some(f) }
	}
}

impl<F> Future for CondvarFuture<F>
where
	// TODO: Is it fine to require `Unpin` here?
	F: FnOnce(&Waker) + Unpin,
{
	type Output = ();

	fn poll(mut self: Pin<&mut Self>, ctx: &mut task::Context<'_>) -> task::Poll<Self::Output> {
		// Check if we still have the function
		match self.f.take() {
			// If we do, call it with the waker and return pending,
			Some(f) => {
				f(ctx.waker());
				task::Poll::Pending
			},

			// Else we've been woken up (possibly spuriously), so return ready
			None => task::Poll::Ready(()),
		}
	}
}
