//! Paths channel
//!
//! Implements a channel to receive file paths from within a root directory.
//!
//! Currently this is only used by the image loaders to
//! receive image paths, but it is made generic to be able to serve
//! other purposes (and maybe one day be moved to it's own crate).
//!
//! The current implementation is made to serve 2 entities:
//! - Receivers
//! - Distributer
//!
//! # Receivers
//! The receivers are the entities that receive paths, they
//! store an instance of the [`Receiver`] type, that allows
//! receiving paths, as well as signaling paths for removal,
//! so they aren't distributed again.
//!
//! The removal feature exists, for example, for when the image loaders encounters
//! a non-image path and wants to signal it to not be distributed again so it doesn't
//! need to re-check the file again.
//!
//! # Distributer
//! The heart of the implementation. Responsible for organizing and distributing
//! all paths for the receivers to receive.
//!
//! Only a single instance of the [`Distributer`] type exists per channel.
//! This instance can then be run until all receivers exit.
//!
//! May also be used to change the channel state, such as the current root directory,
//! what happens when the end of the paths is reached, and others.

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
	// Note: This is set to `true` at the beginning to,
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
