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

use crossbeam::channel::SendTimeoutError;
// Imports
use parking_lot::Mutex;
use rand::prelude::SliceRandom;
use std::{
	mem,
	path::{Path, PathBuf},
	sync::{
		atomic::{self, AtomicBool},
		Arc,
	},
	time::Duration,
};

/// Inner
#[derive(Debug)]
struct Inner {
	/// Root path
	root_path: Arc<PathBuf>,

	/// Cached paths, if any
	// Note: This will be set to `None` at the beginning
	//       and whenever the root path changes. This allows
	//       the file loading to be done by the distributer
	//       thread instead of on the caller thread.
	cached_paths: Option<Vec<Arc<PathBuf>>>,
}

/// A receiver
#[derive(Clone, Debug)]
pub struct Receiver {
	/// Inner
	inner: Arc<Mutex<Inner>>,

	/// Path receiver
	rx: crossbeam::channel::Receiver<Arc<PathBuf>>,
}

impl Receiver {
	/// Receives the next path
	pub fn recv(&self) -> Result<Arc<PathBuf>, DistributerQuitError> {
		self.rx.recv().map_err(|_| DistributerQuitError)
	}

	/// Removes a path
	pub fn remove(&self, path: &Arc<PathBuf>) {
		let mut inner = self.inner.lock();
		if let Some(paths) = &mut inner.cached_paths {
			match paths.iter().position(|cached_path| cached_path == path) {
				// Note: Since we shuffle, it's fine to swap remove
				Some(idx) => {
					#[allow(clippy::let_underscore_drop)] // We want to drop the path
					let _ = paths.swap_remove(idx);
				},
				// Note: This happens whenever paths is modified in between the receive call and the remove call.
				None => log::debug!("Unable to remove path {path:?}, index not found"),
			}
		}
	}
}

/// The distributer
///
/// This type may not be cloned, only a single instance
/// exists per channel.
#[derive(Debug)]
pub struct Distributer {
	/// Inner
	inner: Arc<Mutex<Inner>>,

	/// Path sender
	tx: crossbeam::channel::Sender<Arc<PathBuf>>,
}

impl Distributer {
	/// Runs the distributer until all receivers have quit
	pub fn run(&self, should_quit: &AtomicBool) -> Result<(), anyhow::Error> {
		let mut cur_paths = vec![];
		'run: loop {
			// Retrieve the paths, or load them
			let mut inner = self.inner.lock();
			let cur_root_path = Arc::clone(&inner.root_path);
			let paths = match &inner.cached_paths {
				Some(paths) => paths,
				None => {
					let paths = inner.cached_paths.insert(vec![]);
					Self::load_paths_into(&cur_root_path, paths);
					&*paths
				},
			};

			// Copy all paths and shuffle
			// Note: We also drop the lock after copying, since we don't need
			//       inner anymore
			cur_paths.extend(paths.iter().cloned());
			mem::drop(inner);
			cur_paths.shuffle(&mut rand::thread_rng());

			// Macro to exit if the `should_quit` flag is true
			macro check_should_quit() {
				if should_quit.load(atomic::Ordering::Relaxed) {
					log::debug!("Received quit notification, quitting");
					break 'run;
				}
			}

			// Then send them all
			for mut path in cur_paths.drain(..) {
				// Check if we should quit
				check_should_quit!();

				// If the root path changed in the meantime, reload
				// TODO: `ptr_eq` here? Not sure if it's worth it
				if self.inner.lock().root_path != cur_root_path {
					log::debug!("Root path changed, resetting");
					continue 'run;
				}

				// Else send the path
				// Note: We sleep at most 1 second to ensure we can check the `should_quit` flag.
				// TODO: Find a cleaner solution
				'send: loop {
					match self.tx.send_timeout(path, Duration::from_secs(1)) {
						Ok(()) => break 'send,
						Err(SendTimeoutError::Timeout(sent_path)) => {
							check_should_quit!();
							path = sent_path;
						},
						Err(SendTimeoutError::Disconnected(_)) => {
							log::debug!("All receivers quit, quitting");
							break 'run;
						},
					}
				}
			}
		}

		Ok(())
	}

	/// Loads all paths
	fn load_paths_into(root_path: &Path, paths: &mut Vec<Arc<PathBuf>>) {
		log::info!("Loading all paths from {root_path:?}");

		let ((), duration) = crate::util::measure(|| {
			crate::util::visit_files_dir(root_path, &mut |path| {
				paths.push(Arc::new(path));
				Ok::<(), !>(())
			})
			.into_ok();
		});
		log::debug!("Took {duration:?} to load all paths from {root_path:?}");
	}

	/// Returns the current root path
	pub fn root_path(&self) -> Arc<PathBuf> {
		Arc::clone(&self.inner.lock().root_path)
	}

	/// Sets the root path
	pub fn set_root_path(&self, path: PathBuf) {
		log::info!("Setting root path to {path:?}");

		// Set the root path and clear all paths
		let mut inner = self.inner.lock();
		inner.root_path = Arc::new(path);
		inner.cached_paths = None;
	}
}

/// Error for when the distributer quit
#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("The distributer has quit")]
pub struct DistributerQuitError;

/// Creates a new channel
///
/// The distributer *must* be run for any paths
/// to be received in the [`Receiver`]s.
pub fn new(root_path: PathBuf) -> (Distributer, Receiver) {
	// Create the inner data
	let inner = Inner {
		root_path:    Arc::new(root_path),
		cached_paths: None,
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
