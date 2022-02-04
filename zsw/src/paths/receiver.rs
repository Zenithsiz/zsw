//! Receiver

// Imports
use {
	super::Inner,
	crate::util::{
		extse::{CrossBeamChannelReceiverSE, ParkingLotMutexSe},
		MightBlock,
	},
	parking_lot::Mutex,
	std::{path::PathBuf, sync::Arc},
	zsw_side_effect_macros::side_effect,
};

/// A receiver
#[derive(Clone, Debug)]
pub struct Receiver {
	/// Inner
	pub(super) inner: Arc<Mutex<Inner>>,

	/// Path receiver
	pub(super) rx: crossbeam::channel::Receiver<Arc<PathBuf>>,
}

impl Receiver {
	/// Receives the next path
	///
	/// # Blocking
	/// Blocks until the distributer sends a path via [`Distributer::run`](super::Distributer::run).
	#[side_effect(MightBlock)]
	pub fn recv(&self) -> Result<Arc<PathBuf>, DistributerQuitError> {
		// DEADLOCK: Caller is responsible for avoiding deadlocks
		self.rx
			.recv_se()
			.allow::<MightBlock>()
			.map_err(|_| DistributerQuitError)
	}

	/// Removes a path
	pub fn remove(&self, path: &Arc<PathBuf>) {
		// DEADLOCK: We ensure this lock can't deadlock by not blocking
		//           while locked.
		let mut inner = self.inner.lock_se().allow::<MightBlock>();

		// If the paths need reloading, no use in removing the path
		if inner.reload_cached {
			return;
		}

		// Else remove it
		// TODO: Not require an owned value here
		let _ = inner.cached_paths.remove(path);
	}
}

/// Error for when the distributer quit
#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("The distributer has quit")]
pub struct DistributerQuitError;
