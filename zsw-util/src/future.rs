//! Future helpers

// Imports
use {
	parking_lot::{Condvar, Mutex},
	std::{
		future::Future,
		sync::{
			atomic::{self, AtomicBool},
			Arc,
		},
		task,
	},
};


/// Future runner
///
/// Adapts a future to run on it's own thread, and be cancellable
/// when polling.
#[derive(Debug)]
pub struct FutureRunner {
	/// Signal
	signal: Arc<FutureSignal>,

	/// If we're running
	running: AtomicBool,
}

impl FutureRunner {
	/// Creates a new future runner
	#[must_use]
	pub fn new() -> Self {
		// Create the waker
		Self {
			signal:  Arc::new(FutureSignal::new()),
			running: AtomicBool::new(false),
		}
	}

	/// Executes the future
	///
	/// # Panics
	/// Panics if called more than once
	#[allow(clippy::result_unit_err)] // TODO: Use custom enum to say if we were cancelled
	pub fn run<F>(&self, f: F) -> Result<F::Output, ()>
	where
		F: Future,
	{
		// 'lock' the running bool
		assert!(
			!self.running.swap(true, atomic::Ordering::AcqRel),
			"Cannot run a future runner more than once"
		);

		// Pin the future
		futures::pin_mut!(f);

		// Create the waker
		let waker = task::Waker::from(Arc::clone(&self.signal));
		let mut ctx = task::Context::from_waker(&waker);

		// Then poll it until we should exit
		// Note: On the first loop, `wait` instantly returns for us to loop
		while let FutureSignalStatus::Poll = self.signal.wait() {
			match f.as_mut().poll(&mut ctx) {
				task::Poll::Ready(output) => return Ok(output),
				task::Poll::Pending => (),
			}
		}

		// Exit the signal if we're still waiting
		self.signal.exit();

		Err(())
	}

	/// Stops the future
	pub fn stop(&self) {
		self.signal.exit();
	}
}

impl Default for FutureRunner {
	fn default() -> Self {
		Self::new()
	}
}

impl Drop for FutureRunner {
	fn drop(&mut self) {
		// Stop the future on-drop
		self.stop();
	}
}

/// Signal inner
#[derive(Debug)]
struct FutureSignalInner {
	/// If we should exit
	should_exit: bool,

	/// If the future should be polled
	should_poll: bool,
}

/// Status on signal waiting
enum FutureSignalStatus {
	/// Should poll
	Poll,

	/// Should exit
	Exit,
}

/// Waker signal for [`FuturesRunner`]
#[derive(Debug)]
struct FutureSignal {
	/// Inner
	inner: Mutex<FutureSignalInner>,

	/// Condvar for waiting
	cond_var: Condvar,
}

impl FutureSignal {
	/// Creates a new signal
	fn new() -> Self {
		Self {
			inner:    Mutex::new(FutureSignalInner {
				should_exit: false,
				should_poll: true,
			}),
			cond_var: Condvar::new(),
		}
	}

	/// Waits until the future should be polled, or we should quit
	fn wait(&self) -> FutureSignalStatus {
		// Keep waiting until either `should_poll` or `should_exit` are true
		// DEADLOCK: We'll be woken up in the waker eventually
		let mut inner = self.inner.lock();
		loop {
			match (inner.should_exit, inner.should_poll) {
				// If we should exit, regardless if we should poll, return
				// Note: Doesn't matter if we set `should_poll` to false here
				(true, _) => break FutureSignalStatus::Exit,

				// Else if we should poll, set it to false and return
				(_, true) => {
					inner.should_poll = false;
					break FutureSignalStatus::Poll;
				},

				// Else wait
				_ => self.cond_var.wait(&mut inner),
			}
		}
	}

	/// Sets to exit
	pub fn exit(&self) {
		// Lock, set `should_exit` to `true` and notify
		// DEADLOCK: `Self::wait` only locks it temporarily without blocking
		let mut inner = self.inner.lock();
		inner.should_exit = true;
		let _ = self.cond_var.notify_one();
	}
}

impl task::Wake for FutureSignal {
	fn wake(self: std::sync::Arc<Self>) {
		// Set that we should be polling
		// DEADLOCK: `Self::wait` only locks it temporarily without blocking
		let mut inner = self.inner.lock();
		inner.should_poll = true;

		// Then notify the waiter
		let _ = self.cond_var.notify_one();
	}
}
