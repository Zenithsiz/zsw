//! Mater barrie
//!
//! A barrier with a master and any number of
//! slave barriers, where the master waits for
//! all slaves to answer

// Imports
use {
	core::{
		future,
		mem,
		task::{self, Waker},
	},
	std::sync::{Arc, nonpoison::Mutex},
};

/// Inner
#[derive(Debug)]
struct Inner {
	/// Number of active slaves
	active_slaves: usize,

	/// Master waiting to wake up
	master_waker: Option<Waker>,

	/// All slaves waiting to wake up
	slave_wakers: Vec<Waker>,
}

/// Master barrier
#[derive(Debug)]
pub struct MasterBarrier {
	inner: Arc<Mutex<Inner>>,
}

impl MasterBarrier {
	/// Meets up with all currently active slaves.
	///
	/// If no slaves are active, waits until at least
	/// one slave becomes activated.
	pub async fn meetup_all(&self) {
		let mut registered = false;
		future::poll_fn(|cx| {
			let mut inner = self.inner.lock();

			// If we've already registered ourselves, then we were woken up
			// by the last slave
			// TODO: Deal with potential spurious wake-ups?
			if registered {
				return task::Poll::Ready(());
			}

			// If there's at least 1 active slave, and all the slaves are waiting, wake
			// everyone up and leave
			if inner.active_slaves > 0 && inner.slave_wakers.len() == inner.active_slaves {
				let wakers = mem::take(&mut inner.slave_wakers);
				drop(inner);
				for waker in wakers {
					waker.wake();
				}
				return task::Poll::Ready(());
			}

			// Otherwise, register our waker and wait for the last slave
			inner.master_waker = Some(cx.waker().clone());
			registered = true;
			task::Poll::Pending
		})
		.await;
	}
}

/// Inactive slave barrier
#[derive(Debug)]
pub struct InactiveSlaveBarrier {
	inner: Arc<Mutex<Inner>>,
}

impl InactiveSlaveBarrier {
	/// Activates this into a slave barrier
	#[must_use]
	pub fn activate(&self) -> SlaveBarrier {
		self.inner.lock().active_slaves += 1;
		SlaveBarrier {
			inner: Arc::clone(&self.inner),
		}
	}
}

/// Slave barrier
#[derive(Debug)]
pub struct SlaveBarrier {
	inner: Arc<Mutex<Inner>>,
}

impl SlaveBarrier {
	/// Meets up with the master barrier and all
	///
	pub async fn meetup(&self) {
		let mut registered = false;
		future::poll_fn(|cx| {
			let mut inner = self.inner.lock();

			// If we've already registered ourselves, then we were woken up
			// by the master
			// TODO: Deal with potential spurious wake-ups?
			if registered {
				return task::Poll::Ready(());
			}

			// If we're the last slave and the master was waiting for us,
			// wake everyone up instead (including the master)
			if inner.slave_wakers.len() + 1 == inner.active_slaves &&
				let Some(master_waker) = inner.master_waker.take()
			{
				let wakers = mem::take(&mut inner.slave_wakers);
				drop(inner);

				for waker in wakers {
					waker.wake();
				}
				master_waker.wake();

				return task::Poll::Ready(());
			}


			// Otherwise register and wait for the master
			inner.slave_wakers.push(cx.waker().clone());
			registered = true;
			task::Poll::Pending
		})
		.await;
	}
}

impl Drop for SlaveBarrier {
	fn drop(&mut self) {
		self.inner.lock().active_slaves -= 1;
	}
}

/// Creates a new barrier.
///
/// The master barrier, once awaited, won't wake up
/// until at least one slave barrier is activated,
/// and then all active slave barriers are awaited
#[must_use]
pub fn barrier() -> (MasterBarrier, InactiveSlaveBarrier) {
	let inner = Arc::new(Mutex::new(Inner {
		active_slaves: 0,
		master_waker:  None,
		slave_wakers:  vec![],
	}));

	(
		MasterBarrier {
			inner: Arc::clone(&inner),
		},
		InactiveSlaveBarrier { inner },
	)
}
