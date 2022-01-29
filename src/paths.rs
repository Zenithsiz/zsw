//! Paths channel
//!
//! Implements a channel to receive file paths from within a root directory.
//!
//! Currently this is only used by the image loaders to
//! receive image paths, but it is made generic to be able to serve
//! other purposes (and maybe one day be moved to it's own crate).
//!
//! # [`Receiver`]
//! The receiver type is used to receive paths from the distributer.
//! It may be cloned and moved to other threads.
//!
//! It may also perform some operation on the existing paths, such as removing
//! a path. This is useful when loading images and a path isn't an image.
//! There would be no use in keeping the path around, so it is removed from
//! the path list so it won't be distributed next cycle.
//!
//! # [`Distributer`]
//! The distributer is responsible for distributing paths to all threads.
//! It may not be cloned and must be ran using its' [`run`](Distributer::run)
//! method so receivers may receive paths.

// Modules
mod distributer;
mod receiver;

// Exports
pub use self::{
	distributer::Distributer,
	receiver::{DistributerQuitError, Receiver},
};

// Imports
use parking_lot::Mutex;
use std::{collections::HashSet, path::PathBuf, sync::Arc};

/// Inner
#[derive(Debug)]
struct Inner {
	/// Root path
	root_path: Arc<PathBuf>,

	/// Cached paths.
	cached_paths: HashSet<Arc<PathBuf>>,

	/// If the paths need to be reloaded
	// Note: This is set to `true` at the beginning too,
	//       to load all paths initially
	reload_cached: bool,
}

/// Creates a new channel
///
/// The distributer *must* be run for any paths
/// to be received in the [`Receiver`]s.
pub fn new(root_path: PathBuf) -> (Distributer, Receiver) {
	// Create the inner data
	let inner = Inner {
		root_path:     Arc::new(root_path),
		cached_paths:  HashSet::new(),
		reload_cached: true,
	};
	let inner = Arc::new(Mutex::new(inner));

	// Then the transmission channel
	// Note: We use a capacity of 0, so receivers can start
	//       receiving new paths when changes occur, instead of
	//       having to wait for all files in the channel buffer.
	// TODO: Should we use a channel or just somehow coordinate using
	//       the inner data?
	let (tx, rx) = crossbeam::channel::bounded(0);

	(
		Distributer {
			inner: Arc::clone(&inner),
			tx,
		},
		Receiver { inner, rx },
	)
}
