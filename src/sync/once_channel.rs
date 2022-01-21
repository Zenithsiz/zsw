//! A channel for transmitting a single value


// Imports
use parking_lot::{Condvar, Mutex};
use std::{mem, sync::Arc};

/// Inner channel shared data
#[derive(Debug)]
struct Inner<T> {
	/// The value
	value: Mutex<Option<T>>,

	/// Condition variable
	condvar: Condvar,
}

/// The channel sender
#[derive(Debug)]
pub struct Sender<T> {
	/// Inner
	inner: Arc<Inner<T>>,
}

impl<T> Drop for Sender<T> {
	fn drop(&mut self) {
		// Lock the paths when dropping so we can make sure the receiver doesn't sleep after we drop
		let _value = self.inner.value.lock();

		// Then wake him up
		let _ = self.inner.condvar.notify_one();
	}
}

impl<T> Sender<T> {
	/// Sends the value, destroying the channel
	pub fn send(self, value: T) -> Result<(), SendError<T>> {
		// If receiver quit, return `Err`
		if Arc::strong_count(&self.inner) == 1 {
			return Err(SendError(value));
		}

		// Lock the value
		let mut value_lock = self.inner.value.lock();

		// Insert the value
		*value_lock = Some(value);

		// Then wake him up
		let _ = self.inner.condvar.notify_one();

		// Forget ourselves so we don't run our drop impl, which
		// attempts to get a lock on the value
		mem::drop(value_lock);
		mem::forget(self);

		Ok(())
	}
}

/// The channel receiver
#[derive(Debug)]
pub struct Receiver<T> {
	/// Inner
	inner: Arc<Inner<T>>,
}

impl<T> Receiver<T> {
	/// Receives the value, destroying the channel
	pub fn recv(self) -> Result<T, RecvError> {
		// Lock the value
		let mut value_lock = self.inner.value.lock();

		// If the value is within, return it
		if let Some(value) = value_lock.take() {
			return Ok(value);
		}

		// If sender quit, return `Err`
		// Note: We need to check this while locked, or else we can enter
		//       a sleep we never return from.
		if Arc::strong_count(&self.inner) == 1 {
			return Err(RecvError);
		}

		// Else wait for it and return it, or `Err(())` if it wasn't there
		self.inner.condvar.wait(&mut value_lock);
		value_lock.take().ok_or(RecvError)
	}

	/// Attempts to receive the value
	pub fn try_recv(self) -> Result<T, TryRecvError<T>> {
		// Lock the value
		let mut value_lock = self.inner.value.lock();

		// If the value is within, return it
		if let Some(value) = value_lock.take() {
			return Ok(value);
		}

		// Else check which error we return
		if Arc::strong_count(&self.inner) == 1 {
			return Err(TryRecvError::SenderQuit);
		}

		// Else return err
		mem::drop(value_lock);
		Err(TryRecvError::NotReady(self))
	}
}

/// Send error
#[derive(Debug, thiserror::Error)]
#[error("Receiver has quit")]
pub struct SendError<T>(pub T);

/// Receive error
#[derive(Debug, thiserror::Error)]
#[error("Sender has quit")]
pub struct RecvError;

/// Try receive error
#[derive(Debug, thiserror::Error)]
pub enum TryRecvError<T> {
	/// Sender has quit
	#[error("Sender has quit")]
	SenderQuit,

	/// Sender hasn't send yet
	#[error("Sender isn't ready yet")]
	NotReady(Receiver<T>),
}

/// Creates a new channel
#[must_use]
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
	// Create the shared inner
	let inner = Arc::new(Inner {
		value:   Mutex::new(None),
		condvar: Condvar::new(),
	});

	(
		Sender {
			inner: Arc::clone(&inner),
		},
		Receiver { inner },
	)
}
