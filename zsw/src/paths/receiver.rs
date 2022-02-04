//! Receiver

// Imports
use {
	super::Inner,
	parking_lot::Mutex,
	std::{path::PathBuf, sync::Arc},
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
	pub fn recv(&self) -> Result<Arc<PathBuf>, DistributerQuitError> {
		self.rx.recv().map_err(|_| DistributerQuitError)
	}

	/// Removes a path
	pub fn remove(&self, path: &Arc<PathBuf>) {
		let mut inner = self.inner.lock();

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
