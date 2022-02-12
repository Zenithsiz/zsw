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

/// Never future runner
///
/// Adapts a future returning `!`, so it may run on it's own thread,
/// and be stopped eventually
pub struct NeverFutureRunner {
	/// Signal
	signal: Arc<NeverFutureSignal>,
}

impl NeverFutureRunner {
	/// Creates a new never future fn
	pub fn new() -> Self {
		// Create the waker
		Self {
			signal: Arc::new(NeverFutureSignal::new()),
		}
	}

	/// Executes the future
	pub fn run<F>(&self, f: F)
	where
		F: Future<Output = !> + Send,
	{
		// Pin the future
		// TODO: Don't allocate?
		let mut f = Box::pin(f);

		// Create the waker
		let waker = task::Waker::from(Arc::clone(&self.signal));
		let mut ctx = task::Context::from_waker(&waker);

		// Then poll it until we should exit
		// Note: On the first loop, `wait` instantly returns for us to loop
		while let NeverFutureSignalStatus::Poll = self.signal.wait() {
			match f.as_mut().poll(&mut ctx) {
				task::Poll::Ready(never) => never,
				task::Poll::Pending => (),
			}
		}
	}

	/// Stops the future
	pub fn stop(&self) {
		self.signal.exit();
	}
}

impl Drop for NeverFutureRunner {
	fn drop(&mut self) {
		// Stop the future on-drop
		self.stop();
	}
}

/// Signal inner
struct NeverFutureSignalInner {
	/// If we should exit
	should_exit: bool,

	/// If the future should be polled
	should_poll: bool,
}

/// Status on signal waiting
enum NeverFutureSignalStatus {
	/// Should poll
	Poll,

	/// Should exit
	Exit,
}

/// Waker signal for [`NeverFuturesRunner`]
struct NeverFutureSignal {
	/// Inner
	inner: Mutex<NeverFutureSignalInner>,

	/// Condvar for waiting
	cond_var: Condvar,
}

impl NeverFutureSignal {
	/// Creates a new signal
	fn new() -> Self {
		Self {
			inner:    Mutex::new(NeverFutureSignalInner {
				should_exit: false,
				should_poll: true,
			}),
			cond_var: Condvar::new(),
		}
	}

	/// Waits until the future should be polled, or we should quit
	pub fn wait(&self) -> NeverFutureSignalStatus {
		// Keep waiting until either `should_poll` or `should_exit` are true
		// DEADLOCK: We'll be woken up in the waker eventually
		let mut inner = self.inner.lock_se().allow::<MightBlock>();
		loop {
			match (inner.should_exit, inner.should_poll) {
				// If we should exit, regardless if we should poll, return
				// Note: Doesn't matter if we set `should_poll` to false here
				(true, _) => break NeverFutureSignalStatus::Exit,

				// Else if we should poll, set it to false and return
				(_, true) => {
					inner.should_poll = false;
					break NeverFutureSignalStatus::Poll;
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

impl task::Wake for NeverFutureSignal {
	fn wake(self: std::sync::Arc<Self>) {
		// Set that we should be polling
		// DEADLOCK: `Self::wait` only locks it temporarily without blocking
		let mut inner = self.inner.lock_se().allow::<MightBlock>();
		inner.should_poll = true;

		// Then notify the waiter
		let _ = self.cond_var.notify_one();
	}
}
