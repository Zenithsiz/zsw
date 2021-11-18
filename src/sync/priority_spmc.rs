//! Priority-based single producer multiple consumer channel

// Imports
use parking_lot::{Condvar, Mutex};
use std::{collections::BinaryHeap, sync::Arc};

/// A value by priority
#[derive(Debug)]
struct ValueByPriority<T> {
	/// Value
	value: T,

	/// Priority
	priority: usize,
}

impl<T> PartialEq for ValueByPriority<T> {
	fn eq(&self, other: &Self) -> bool {
		self.priority == other.priority
	}
}

impl<T> Eq for ValueByPriority<T> {}

impl<T> PartialOrd for ValueByPriority<T> {
	fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
		Some(self.cmp(other))
	}
}

impl<T> Ord for ValueByPriority<T> {
	fn cmp(&self, other: &Self) -> std::cmp::Ordering {
		self.priority.cmp(&other.priority)
	}
}

/// Channel buffer
#[derive(Debug)]
struct Buffer<T> {
	/// Queue
	queue: BinaryHeap<ValueByPriority<T>>,
}

impl<T> Buffer<T> {
	/// Creates an empty buffer
	pub fn empty() -> Self {
		Self {
			queue: BinaryHeap::new(),
		}
	}
}

/// Inner channel shared data
#[derive(Debug)]
struct Inner<T> {
	/// The buffer
	buffer: Mutex<Buffer<T>>,

	/// Receiver condition variable
	condvar: Condvar,
}

/// A channel sender
#[derive(Debug)]
pub struct Sender<T> {
	/// Inner
	inner: Arc<Inner<T>>,
}

impl<T> Drop for Sender<T> {
	fn drop(&mut self) {
		// Lock the buffer and notify the other end that we're quitting
		let _buffer = self.inner.buffer.lock();
		self.inner.condvar.notify_all();
	}
}

impl<T> Sender<T> {
	/// Sends a value
	pub fn send(&self, value: T, priority: usize) -> Result<(), SendError> {
		// Lock the buffer
		let mut buffer = self.inner.buffer.lock();

		// If all receivers quit, return Err
		if Arc::strong_count(&self.inner) == 1 {
			return Err(SendError);
		}

		// Else insert the value
		buffer.queue.push(ValueByPriority { value, priority });

		// Then wake a receiver
		self.inner.condvar.notify_one();

		Ok(())
	}
}

/// Error for `Sender::send`
#[derive(Debug, thiserror::Error)]
#[error("All receivers quit")]
pub struct SendError;

/// A channel receiver
#[derive(Debug)]
pub struct Receiver<T> {
	/// Inner
	inner: Arc<Inner<T>>,
}

impl<T> Clone for Receiver<T> {
	fn clone(&self) -> Self {
		Self {
			inner: Arc::clone(&self.inner),
		}
	}
}

impl<T> Receiver<T> {
	/// Receives a value
	pub fn recv(&self) -> Result<T, RecvError> {
		// Lock the buffer
		let mut buffer = self.inner.buffer.lock();

		// Get the request
		match buffer.queue.pop() {
			// If we had it, return it
			Some(value) => Ok(value.value),

			// Else make sure the requested is still alive and wait for it
			None => {
				// Else if the sender quit, return Err
				if Arc::strong_count(&self.inner) == 1 {
					return Err(RecvError);
				}

				// Else wait until we get a notification
				self.inner.condvar.wait(&mut buffer);

				// Then get it, or return `Err` if they quit
				buffer.queue.pop().map(|value| value.value).ok_or(RecvError)
			},
		}
	}

	/// Tries to receive a value
	#[allow(dead_code)] // It might be useful eventually
	pub fn try_recv(&self) -> Result<T, TryRecvError> {
		// Lock the buffer
		let mut buffer = self.inner.buffer.lock();

		// Get the request
		match buffer.queue.pop() {
			// If we had it, return it
			Some(value) => Ok(value.value),

			// Else make sure the requested is still alive and return
			None => {
				// Else if the sender quit, return Err
				if Arc::strong_count(&self.inner) == 1 {
					Err(TryRecvError::SenderQuit)
				} else {
					Err(TryRecvError::NotReady)
				}
			},
		}
	}
}

/// Error for `Receiver::recv`
#[derive(Debug, thiserror::Error)]
#[error("Sender quit")]
pub struct RecvError;

/// Error for `Receiver::try_recv`
#[derive(Debug, thiserror::Error)]
pub enum TryRecvError {
	/// Sender quit
	#[error("Sender quit")]
	SenderQuit,

	/// Not ready
	#[error("Not ready")]
	NotReady,
}

/// Creates a new channel
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
	// Create the shared inner
	let inner = Arc::new(Inner {
		buffer:  Mutex::new(Buffer::empty()),
		condvar: Condvar::new(),
	});

	(
		Sender {
			inner: Arc::clone(&inner),
		},
		Receiver { inner },
	)
}
