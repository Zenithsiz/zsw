//! Meetup channel

// Imports
use {
	futures::{lock::Mutex, FutureExt},
	std::{
		sync::Arc,
		task::{Poll, Waker},
	},
};

/// Inner
struct Inner<T> {
	/// Value
	value: Option<T>,

	/// Wakers waiting for a `recv`
	recv_wakers: Vec<Waker>,

	/// Wakers waiting for a `send`
	send_wakers: Vec<Waker>,
}

/// Sender
#[derive(Debug)]
pub struct Sender<T> {
	/// Inner
	inner: Arc<Mutex<Inner<T>>>,
}

impl<T> Sender<T> {
	/// Sends a value.
	///
	/// Blocks until the value is received
	#[allow(clippy::disallowed_methods)] // DEADLOCK: We ensure thread safety by only locking the mutex temporarily
	pub async fn send(&self, value: T) {
		let mut value = Some(value);
		let mut inner_lock_fut = self.inner.lock();
		std::future::poll_fn(|cx| {
			// Lock the mutex
			// Note: After locking we need to ensure that we re-create the lock future, as it's exhausted
			let mut inner = inner_lock_fut.poll_unpin(cx).ready()?;
			inner_lock_fut = self.inner.lock();

			// If the value is present, add ourselves to the wakers and return pending
			if inner.value.is_some() {
				inner.recv_wakers.push(cx.waker().clone());
				return Poll::Pending;
			}

			// Else set the value and wake all send wakers
			inner.value = Some(value.take().expect("Future was polled after exhausted"));
			inner.send_wakers.drain(..).for_each(Waker::wake);

			Poll::Ready(())
		})
		.await;
	}
}

/// Receiver
#[derive(Debug)]
pub struct Receiver<T> {
	/// Inner
	inner: Arc<Mutex<Inner<T>>>,
}

impl<T> Receiver<T> {
	/// Receives the next value
	///
	/// Blocks until the value is sent
	#[allow(clippy::disallowed_methods)] // DEADLOCK: We ensure thread safety by only locking the mutex temporarily
	pub async fn recv(&self) -> T {
		let mut inner_lock_fut = self.inner.lock();
		std::future::poll_fn(|cx| {
			// Lock the mutex
			// Note: After locking we need to ensure that we re-create the lock future, as it's exhausted
			let mut inner = inner_lock_fut.poll_unpin(cx).ready()?;
			inner_lock_fut = self.inner.lock();

			// If the value isn't present, add ourselves to the wakers and return pending
			let Some(value) = inner.value.take() else {
				inner.send_wakers.push(cx.waker().clone());
				return Poll::Pending;
			};

			// Then wake all senders awaiting for us
			inner.recv_wakers.drain(..).for_each(Waker::wake);

			Poll::Ready(value)
		})
		.await
	}

	/// Tries to receive the next value
	pub fn try_recv(&self) -> Option<T> {
		let mut inner = self.inner.try_lock()?;

		let value = inner.value.take()?;
		inner.recv_wakers.drain(..).for_each(Waker::wake);

		Some(value)
	}
}

/// Creates a new channel
#[must_use]
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
	let inner = Inner {
		value:       None,
		recv_wakers: vec![],
		send_wakers: vec![],
	};
	let inner = Arc::new(Mutex::new(inner));

	(
		Sender {
			inner: Arc::clone(&inner),
		},
		Receiver { inner },
	)
}
