//! Priority-based single producer multiple consumer channel

// Imports
use parking_lot::{Condvar, Mutex};
use std::{collections::BinaryHeap, num::NonZeroUsize, sync::Arc};

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

	/// Queue max capacity
	capacity: Option<NonZeroUsize>,
}

impl<T> Buffer<T> {
	/// Creates an empty buffer
	pub fn empty(capacity: Option<NonZeroUsize>) -> Self {
		Self {
			queue: BinaryHeap::new(),
			capacity,
		}
	}
}

/// Inner channel shared data
#[derive(Debug)]
struct Inner<T> {
	/// The buffer
	buffer: Mutex<Buffer<T>>,

	/// Receiver condition variable
	receiver_condvar: Condvar,

	/// Sender condition variable
	sender_condvar: Condvar,
}

/// A channel sender
#[derive(Debug)]
pub struct Sender<T> {
	/// Inner
	inner: Arc<Inner<T>>,
}

impl<T> Drop for Sender<T> {
	fn drop(&mut self) {
		// Lock the buffer and notify all receivers that we're quitting
		let _buffer = self.inner.buffer.lock();
		self.inner.receiver_condvar.notify_all();
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

		// Check the buffer's capacity
		loop {
			match buffer.capacity {
				// If none, or we're within the limit, continue
				None => break,
				Some(capacity) if buffer.queue.len() < capacity.get() => break,

				// Else wait on the condvar for a receiver to wake us up
				Some(_) => self.inner.sender_condvar.wait(&mut buffer),
			}
		}

		// Then insert the value
		buffer.queue.push(ValueByPriority { value, priority });

		// And wake a receiver
		self.inner.receiver_condvar.notify_one();

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
		let value = match buffer.queue.pop() {
			// If we had it, return it
			Some(value) => value.value,

			// Else make sure the requested is still alive and wait for it
			None => {
				// Else if the sender quit, return Err
				if Arc::strong_count(&self.inner) == 1 {
					return Err(RecvError);
				}

				// Else wait until the sender sends a value
				self.inner.receiver_condvar.wait(&mut buffer);

				// Then get it, or return `Err` if they quit
				buffer.queue.pop().ok_or(RecvError)?.value
			},
		};

		// Then wake up the sender if they're waiting and return
		self.inner.sender_condvar.notify_one();
		Ok(value)
	}

	/// Tries to receive a value
	#[allow(dead_code)] // It might be useful eventually
	pub fn try_recv(&self) -> Result<T, TryRecvError> {
		// Lock the buffer
		let mut buffer = self.inner.buffer.lock();

		// Try to get the request
		match buffer.queue.pop() {
			// If we had it, wake up the sender and return
			Some(value) => {
				self.inner.sender_condvar.notify_one();
				Ok(value.value)
			},

			// Else make sure the requested is still alive and return
			None => match Arc::strong_count(&self.inner) {
				1 => Err(TryRecvError::SenderQuit),
				_ => Err(TryRecvError::NotReady),
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
pub fn channel<T>(capacity: Option<NonZeroUsize>) -> (Sender<T>, Receiver<T>) {
	// Create the shared inner
	let inner = Arc::new(Inner {
		buffer:           Mutex::new(Buffer::empty(capacity)),
		receiver_condvar: Condvar::new(),
		sender_condvar:   Condvar::new(),
	});

	(
		Sender {
			inner: Arc::clone(&inner),
		},
		Receiver { inner },
	)
}
