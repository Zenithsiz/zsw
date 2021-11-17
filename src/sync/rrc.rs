//! Request-Response channel
//!
//! A one-to-many channel where the sole end may "request" an operation, then block
//! until a "response" is emitted from the other end.
//!
//! Given that this is a one-to-many channel, the messages sent should likely just be a
//! signal to start doing something, possibly returning a "token" to receive the result
//! of the operation, rather then performing the operation as the request.

// Imports
use parking_lot::{Condvar, Mutex};
use std::sync::Arc;

/// Channel buffer
#[derive(Debug)]
struct Buffer<Req, Res> {
	/// Request
	request: Option<Req>,

	/// Response
	response: Option<Res>,
}

impl<Req, Res> Buffer<Req, Res> {
	/// Creates an empty buffer
	pub const fn empty() -> Self {
		Self {
			request:  None,
			response: None,
		}
	}
}

/// Inner channel shared data
#[derive(Debug)]
struct Inner<Req, Res> {
	/// The buffer
	buffer: Mutex<Buffer<Req, Res>>,

	/// Requester condition variable
	requester_condvar: Condvar,

	/// Responder condition variable
	responder_condvar: Condvar,
}

/// A request-response channel requester
#[derive(Debug)]
pub struct Requester<Req, Res> {
	/// Inner
	inner: Arc<Inner<Req, Res>>,
}

impl<Req, Res> Drop for Requester<Req, Res> {
	fn drop(&mut self) {
		// Lock the buffer and notify the other end that we're quitting
		let _buffer = self.inner.buffer.lock();
		self.inner.responder_condvar.notify_all();
	}
}

impl<Req, Res> Requester<Req, Res> {
	/// Posts a new request and waits until responded
	pub fn request(&self, request: Req) -> Result<Res, RequestError> {
		// Lock the buffer
		let mut buffer = self.inner.buffer.lock();

		// If the responder quit, return Err
		if Arc::strong_count(&self.inner) == 1 {
			return Err(RequestError);
		}

		// Else insert the request
		buffer.request = Some(request);

		// Then wake a responder, and wait until they respond
		self.inner.responder_condvar.notify_one();
		self.inner.requester_condvar.wait(&mut buffer);

		// Then get the response, or return an error if they didn't respond
		buffer.response.take().ok_or(RequestError)
	}
}

/// Error for `request_wait`
#[derive(Debug, thiserror::Error)]
#[error("All responders quit")]
pub struct RequestError;

/// A request-response channel responder
#[derive(Debug)]
pub struct Responder<Req, Res> {
	/// Inner
	inner: Arc<Inner<Req, Res>>,
}

impl<Req, Res> Clone for Responder<Req, Res> {
	fn clone(&self) -> Self {
		Self {
			inner: Arc::clone(&self.inner),
		}
	}
}

impl<Req, Res> Drop for Responder<Req, Res> {
	fn drop(&mut self) {
		// Lock the buffer and notify the other end that we're quitting
		let _buffer = self.inner.buffer.lock();
		self.inner.requester_condvar.notify_one();
	}
}

impl<Req, Res> Responder<Req, Res> {
	/// Waits for a request, and responds to it using `f`
	pub fn respond<T>(&self, f: impl FnOnce(Req) -> (T, Res)) -> Result<T, RespondError> {
		// Lock the buffer
		let mut buffer = self.inner.buffer.lock();

		// Get the request
		let request = match buffer.request.take() {
			// If we had it, use it
			Some(request) => request,

			// Else make sure the requested is still alive and wait for it
			None => {
				// Else if the requester quit, return Err
				if Arc::strong_count(&self.inner) == 1 {
					return Err(RespondError);
				}

				// Else wait until we get a notification
				self.inner.responder_condvar.wait(&mut buffer);

				// Then get it, or return `Err` if they quit
				buffer.request.take().ok_or(RespondError)?
			},
		};

		// Then respond
		let (value, response) = f(request);
		buffer.response = Some(response);

		// And notify the requester
		self.inner.requester_condvar.notify_one();

		Ok(value)
	}
}

/// Error for `wait_request`
#[derive(Debug, thiserror::Error)]
#[error("Requester quit")]
pub struct RespondError;

/// Creates a new channel
pub fn channel<Req, Res>() -> (Requester<Req, Res>, Responder<Req, Res>) {
	// Create the shared inner
	let inner = Arc::new(Inner {
		buffer:            Mutex::new(Buffer::empty()),
		requester_condvar: Condvar::new(),
		responder_condvar: Condvar::new(),
	});

	(
		Requester {
			inner: Arc::clone(&inner),
		},
		Responder { inner },
	)
}
