//! Path loader

// Imports
use crate::{sync::priority_spmc, util};
use anyhow::Context;
use notify::Watcher;
use rand::prelude::SliceRandom;
use std::{
	num::NonZeroUsize,
	ops::ControlFlow,
	path::{Path, PathBuf},
	sync::{mpsc, Arc},
	thread,
	time::Duration,
};

/// The path loader
pub struct PathLoader {
	/// Receiver for the path
	path_receiver: priority_spmc::Receiver<Arc<PathBuf>>,

	/// Filesystem event sender
	fs_sender: mpsc::Sender<notify::DebouncedEvent>,

	/// Filesystem watcher
	_fs_watcher: notify::RecommendedWatcher,
}

impl PathLoader {
	/// Creates a new path loader
	///
	/// # Errors
	/// Returns an error if unable to start watching the filesystem, or if unable to start
	/// the path loader thread
	pub fn new(base_path: PathBuf) -> Result<Self, anyhow::Error> {
		// Start the filesystem watcher and start watching the path
		let (fs_sender, fs_receiver) = mpsc::channel();
		let mut fs_watcher =
			notify::watcher(fs_sender.clone(), Duration::from_secs(2)).context("Unable to create directory watcher")?;
		fs_watcher
			.watch(&base_path, notify::RecursiveMode::Recursive)
			.context("Unable to start watching directory")?;

		// Create both channels
		// Note: Since we can hand out paths quickly, we can use a relatively low capacity
		#[allow(clippy::expect_used)] // It won't panic
		let (path_sender, path_receiver) = priority_spmc::channel(Some(NonZeroUsize::new(16).expect("16 isn't 0")));

		// Then start the path loader thread
		thread::Builder::new()
			.name("Path loader".to_owned())
			.spawn(
				move || match self::loader_thread(&base_path, &fs_receiver, &path_sender) {
					Ok(()) => log::debug!("Path loader successfully returned"),
					Err(err) => log::error!("Path loader returned an error: {err:?}"),
				},
			)
			.context("Unable to start path loader thread")?;

		Ok(Self {
			path_receiver,
			fs_sender,
			_fs_watcher: fs_watcher,
		})
	}

	/// Returns a receiver for paths
	pub fn receiver(&self) -> PathReceiver {
		PathReceiver {
			path_receiver: self.path_receiver.clone(),
			fs_sender:     self.fs_sender.clone(),
		}
	}
}

/// A path receiver
pub struct PathReceiver {
	/// Receiver for the path
	path_receiver: priority_spmc::Receiver<Arc<PathBuf>>,

	/// Filesystem event sender
	fs_sender: mpsc::Sender<notify::DebouncedEvent>,
}

impl PathReceiver {
	/// Receives a path
	pub fn recv(&self) -> Result<Arc<PathBuf>, RecvError> {
		self.path_receiver.recv().map_err(|_| RecvError)
	}

	/// Tries to receive a value
	#[allow(dead_code)] // It might be useful eventually
	pub fn try_recv(&self) -> Result<Arc<PathBuf>, TryRecvError> {
		self.path_receiver.try_recv().map_err(|err| match err {
			priority_spmc::TryRecvError::SenderQuit => TryRecvError::LoaderQuit,
			priority_spmc::TryRecvError::NotReady => TryRecvError::NotReady,
		})
	}

	/// Reports a path for removal
	pub fn remove_path(&self, path: PathBuf) -> Result<(), RemovePathError> {
		self.fs_sender
			.send(notify::DebouncedEvent::Remove(path))
			.map_err(|_| RemovePathError)
	}
}

/// Error for [`PathReceiver::recv`]
#[derive(Debug, thiserror::Error)]
#[error("Path loader thread quit")]
pub struct RecvError;

/// Error for [`PathReceiver::try_recv`]
#[derive(Debug, thiserror::Error)]
pub enum TryRecvError {
	/// Loader thread quit
	#[error("Path loader thread quit")]
	LoaderQuit,

	/// Not ready
	#[error("Not ready")]
	NotReady,
}
/// Error for [`PathReceiver::remove_path`]
#[derive(Debug, thiserror::Error)]
#[error("Path loader thread quit")]
pub struct RemovePathError;


/// Path loader thread
fn loader_thread(
	base_path: &Path, fs_rx: &mpsc::Receiver<notify::DebouncedEvent>, path_sender: &priority_spmc::Sender<Arc<PathBuf>>,
) -> Result<(), anyhow::Error> {
	// Load all existing paths
	let mut paths = vec![];
	self::scan_dir(base_path, &mut paths);

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
			// Note: Priority for the path sender isn't mega relevant for now
			if path_sender.send(path, 0).is_err() {
				return Ok(());
			}
		}
	}
}

/// Handles a filesystem event
fn handle_fs_event(event: notify::DebouncedEvent, base_path: &Path, paths: &mut Vec<Arc<PathBuf>>) {
	log::trace!("Receive filesystem event: {event:?}");

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
		notify::DebouncedEvent::Rescan => {
			log::warn!("Re-scanning");
			paths.clear();
			self::scan_dir(base_path, paths);
		},

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

/// Scans `base_path` to `paths`
fn scan_dir(base_path: &Path, paths: &mut Vec<Arc<PathBuf>>) {
	let paths_loaded = util::visit_files_dir::<!, _>(base_path, &mut |path| {
		paths.push(Arc::new(path));
		ControlFlow::CONTINUE
	})
	.into_ok();
	log::info!("Loaded {paths_loaded} paths");
}