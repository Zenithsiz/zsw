//! Threading utilities

// Imports
use {
	super::{extse::ParkingLotMutexSe, MightBlock},
	anyhow::Context,
	crossbeam::thread::{Scope, ScopedJoinHandle},
	parking_lot::{Condvar, Mutex},
	std::{future::Future, sync::Arc, task},
};

/// Thread spawned
pub struct ThreadSpawner<'scope, 'env, T> {
	/// Scope
	scope: &'scope Scope<'env>,

	/// All join handles along with the thread names
	join_handles: Vec<ScopedJoinHandle<'scope, T>>,
}

impl<'scope, 'env, T> ThreadSpawner<'scope, 'env, T> {
	/// Creates a new thread spawner
	pub fn new(scope: &'scope Scope<'env>) -> Self {
		Self {
			scope,
			join_handles: vec![],
		}
	}

	/// Spawns a new thread
	pub fn spawn<F>(&mut self, name: impl Into<String>, f: F) -> Result<(), anyhow::Error>
	where
		F: Send + FnOnce() -> T + 'env,
		T: Send + 'env,
	{
		let name = name.into();
		let handle = self
			.scope
			.builder()
			.name(name.clone())
			.spawn(|_| f())
			.with_context(|| format!("Unable to start thread {name:?}"))?;
		self.join_handles.push(handle);

		Ok(())
	}

	/// Joins all threads
	pub fn join_all<C: FromIterator<T>>(self) -> Result<C, anyhow::Error> {
		// Note: We only join in reverse order because that's usually the order
		//       the threads will stop, nothing else. No sequencing exists.
		self.join_handles
			.into_iter()
			.rev()
			.map(|handle| {
				let name = handle.thread().name().unwrap_or("<unnamed>").to_owned();
				log::debug!("Joining thread '{name:?}'");
				handle
					.join()
					.map_err(|err| anyhow::anyhow!("Thread '{name}' panicked at {err:?}"))
			})
			.collect()
	}
}

/// Future runner
///
/// Adapts a future to run on it's own thread, and be cancellable
/// when polling.
pub struct FutureRunner {
	/// Signal
	signal: Arc<FutureSignal>,
}

impl FutureRunner {
	/// Creates a new future runner
	pub fn new() -> Self {
		// Create the waker
		Self {
			signal: Arc::new(FutureSignal::new()),
		}
	}

	/// Executes the future
	pub fn run<F>(&self, f: F) -> Result<F::Output, ()>
	where
		F: Future,
	{
		// Pin the future
		// TODO: Don't allocate?
		let mut f = Box::pin(f);

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

		Err(())
	}

	/// Stops the future
	pub fn stop(&self) {
		self.signal.exit();
	}
}

impl Drop for FutureRunner {
	fn drop(&mut self) {
		// Stop the future on-drop
		self.stop();
	}
}

/// Signal inner
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
	pub fn wait(&self) -> FutureSignalStatus {
		// Keep waiting until either `should_poll` or `should_exit` are true
		// DEADLOCK: We'll be woken up in the waker eventually
		let mut inner = self.inner.lock_se().allow::<MightBlock>();
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
		let mut inner = self.inner.lock_se().allow::<MightBlock>();
		inner.should_exit = true;
		let _ = self.cond_var.notify_one();
	}
}

impl task::Wake for FutureSignal {
	fn wake(self: std::sync::Arc<Self>) {
		// Set that we should be polling
		// DEADLOCK: `Self::wait` only locks it temporarily without blocking
		let mut inner = self.inner.lock_se().allow::<MightBlock>();
		inner.should_poll = true;

		// Then notify the waiter
		let _ = self.cond_var.notify_one();
	}
}
