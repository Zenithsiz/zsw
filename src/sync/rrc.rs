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

impl<Req, Res> Requester<Req, Res> {
	/// Posts a new request and waits until responded
	pub fn request_wait(&self, request: Req) -> Res {
		// Lock the buffer
		let mut buffer = self.inner.buffer.lock();

		// Insert the request
		buffer.request = Some(request);

		// Then wake a responder, and wait until they respond
		self.inner.responder_condvar.notify_one();
		self.inner.requester_condvar.wait(&mut buffer);

		// Then get the response
		#[allow(clippy::expect_used)] // This is an internal assertion and shouldn't happen
		buffer.response.take().expect("Responder didn't leave a response")
	}
}

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

impl<Req, Res> Responder<Req, Res> {
	/// Waits for a request, and responds to it using `f`
	pub fn wait_request<T>(&self, f: impl FnOnce(Req) -> (T, Res)) -> T {
		// Lock the buffer
		let mut buffer = self.inner.buffer.lock();

		// Then wait until we get a notification
		self.inner.responder_condvar.wait(&mut buffer);

		// Once we get one, respond
		#[allow(clippy::expect_used)] // This is an internal assertion and shouldn't happen
		let request = buffer.request.take().expect("Requester didn't leave a request");
		let (value, response) = f(request);
		buffer.response = Some(response);

		// And notify the requester
		self.inner.requester_condvar.notify_one();

		value
	}
}

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
