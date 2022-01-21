//! Paths
//!
//! See the [`Paths`] type for more details.

// Modules
mod error;

// Exports
pub use error::NewError;

// Imports
use crate::util;
use notify::Watcher;
use rand::prelude::SliceRandom;
use std::{
	path::{Path, PathBuf},
	sync::{mpsc, Arc},
	thread,
	time::Duration,
};

/// Paths
///
/// A low-latency generator of paths to load.
///
/// This is intended as a low-contention channel, that is,
/// it is not optimized for distribution speed.
pub struct Paths {
	/// Receiver for the path
	path_rx: crossbeam::channel::Receiver<Arc<PathBuf>>,

	/// Filesystem event sender
	fs_tx: mpsc::Sender<notify::DebouncedEvent>,

	/// Filesystem watcher
	_fs_watcher: notify::RecommendedWatcher,
}

impl std::fmt::Debug for Paths {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("PathLoader")
			.field("path_rx", &self.path_rx)
			.field("fs_tx", &self.fs_tx)
			.field("_fs_watcher", &"..")
			.finish()
	}
}

impl Paths {
	/// Creates a new path manager and starts loading paths in the background.
	pub fn new(base_path: PathBuf) -> Result<Self, NewError> {
		// Start the filesystem watcher and start watching the path
		let (fs_tx, fs_rx) = mpsc::channel();
		let mut fs_watcher =
			notify::watcher(fs_tx.clone(), Duration::from_secs(2)).map_err(NewError::CreateFsWatcher)?;
		fs_watcher
			.watch(&*base_path, notify::RecursiveMode::Recursive)
			.map_err(NewError::WatchFilesystemDir)?;

		// Then start loading all existing path
		{
			let base_path = base_path.clone();
			let fs_sender = fs_tx.clone();
			let _loader_thread = thread::Builder::new()
				.name("Paths initial-loader".to_owned())
				.spawn(move || self::load_paths(&base_path, &fs_sender))
				.map_err(NewError::CreateLoaderThread)?;
		}

		// Create both channels
		// Note: Since we have relatively low contention, 16 is more than enough capacity,
		//       we just don't make it 0 so that threads don't have to wait for the distributer
		//       thread to wake up to send them a path and can instead just queue the next whenever
		//       it wakes up.
		let (path_tx, path_rx) = crossbeam::channel::bounded(16);

		// Then start the path distributer thread
		let _distributer_thread = thread::Builder::new()
			.name("Paths distributor".to_owned())
			.spawn(move || match self::distributer_thread(&base_path, &fs_rx, &path_tx) {
				Ok(()) => log::debug!("Path distributor successfully returned"),
				Err(err) => log::error!("Path distributor returned an error: {err:?}"),
			})
			.map_err(NewError::CreateDistributerThread)?;

		Ok(Self {
			path_rx,
			fs_tx,
			_fs_watcher: fs_watcher,
		})
	}

	/// Returns a receiver for paths
	pub fn receiver(&self) -> PathReceiver {
		PathReceiver {
			path_rx: self.path_rx.clone(),
			fs_tx:   self.fs_tx.clone(),
		}
	}
}

/// A path receiver
#[derive(Debug)]
pub struct PathReceiver {
	/// Receiver for the path
	path_rx: crossbeam::channel::Receiver<Arc<PathBuf>>,

	/// Filesystem event sender
	fs_tx: mpsc::Sender<notify::DebouncedEvent>,
}

impl PathReceiver {
	/// Receives a path
	pub fn recv(&self) -> Result<Arc<PathBuf>, RecvError> {
		self.path_rx.recv().map_err(|_| RecvError)
	}

	// Note: `try_recv` isn't super useful, since we're low-latency, so we
	//       just don't offer it.

	/// Reports a path for removal
	pub fn remove_path(&self, path: PathBuf) -> Result<(), RemovePathError> {
		// TODO: Ideally we wouldn't hijack the filesystem watcher
		//       events for this and we'd create a custom event channel,
		//       but not worth it for just this. When we expand the path distributer
		//       to support adding/removing paths we'll migrate to that.
		self.fs_tx
			.send(notify::DebouncedEvent::Remove(path))
			.map_err(|_| RemovePathError)
	}
}

/// Error for [`PathReceiver::recv`]
#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("Paths distributer thread quit")]
pub struct RecvError;

/// Error for [`PathReceiver::remove_path`]
#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("Paths distributer thread quit")]
pub struct RemovePathError;

/// Path distributer thread
fn distributer_thread(
	base_path: &Path, fs_rx: &mpsc::Receiver<notify::DebouncedEvent>,
	path_sender: &crossbeam::channel::Sender<Arc<PathBuf>>,
) -> Result<(), anyhow::Error> {
	// Load all existing paths in a background thread
	let mut paths = vec![];


	loop {
		// Check if we have any filesystem events
		while let Ok(event) = fs_rx.try_recv() {
			self::handle_fs_event(event, base_path, &mut paths);
		}

		// If we have no paths, wait for a filesystem event, or return, if unable to
		while paths.is_empty() {
			log::warn!("No paths found, waiting for new files from the filesystem watcher");
			match fs_rx.recv() {
				Ok(event) => self::handle_fs_event(event, base_path, &mut paths),
				Err(_) => anyhow::bail!("No paths are available and the filesystem watcher closed their channel"),
			}
		}

		// Then shuffle the paths we have
		log::trace!("Shuffling all files");
		paths.shuffle(&mut rand::thread_rng());

		// Then send all paths through the sender
		for path in paths.iter().map(Arc::clone) {
			// Send it and quit if we're done
			if path_sender.send(path).is_err() {
				return Ok(());
			}
		}
	}
}

/// Handles a filesystem event
fn handle_fs_event(event: notify::DebouncedEvent, _base_path: &Path, paths: &mut Vec<Arc<PathBuf>>) {
	#[allow(clippy::match_same_arms)] // They're logically in different parts
	match event {
		// Add the path
		notify::DebouncedEvent::Create(path) => {
			log::debug!("Adding {path:?}");
			paths.push(Arc::new(path));
		},
		// Replace the path
		notify::DebouncedEvent::Rename(old_path, new_path) => {
			log::debug!("Renaming {old_path:?} to {new_path:?}");
			for path in paths {
				if **path == old_path {
					*path = Arc::new(new_path);
					break;
				}
			}
		},
		// Remove the path
		notify::DebouncedEvent::Remove(path_to_remove) => {
			log::debug!("Removing {path_to_remove:?}");
			paths.retain(|path| **path != path_to_remove);
		},

		// Clear all paths and rescan
		notify::DebouncedEvent::Rescan => log::warn!("Re-scanning (Not yet implemented)"),

		// Note: Ignore any R/W events
		// TODO: Check if we should be doing this?
		notify::DebouncedEvent::NoticeWrite(_) |
		notify::DebouncedEvent::NoticeRemove(_) |
		notify::DebouncedEvent::Write(_) |
		notify::DebouncedEvent::Chmod(_) => (),

		// Log the error
		notify::DebouncedEvent::Error(err, path) => match path {
			Some(path) => log::warn!("Found error for path {path:?}: {:?}", anyhow::anyhow!(err)),
			None => log::warn!("Found error for unknown path: {:?}", anyhow::anyhow!(err)),
		},
	}
}

/// Loads all paths from `base_path` and sends them to `fs_tx`
fn load_paths(base_path: &Path, fs_tx: &mpsc::Sender<notify::DebouncedEvent>) {
	let mut paths_loaded = 0;
	let (res, loading_duration) = util::measure(|| {
		util::visit_files_dir(base_path, &mut |path| {
			paths_loaded += 1;
			fs_tx.send(notify::DebouncedEvent::Create(path))
		})
	});

	match res {
		Ok(()) => log::debug!("Finishing loading all {paths_loaded} paths in {loading_duration:?}"),
		Err(_) => log::warn!("Stopping loading of paths due to receiver quitting"),
	}
}
