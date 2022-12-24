//! Image playlist
//!
//! Manages order of images to load.

// Features
#![feature(never_type)]

// Modules
pub mod manager;
pub mod receiver;
pub mod runner;

// Exports
pub use {manager::PlaylistManager, receiver::PlaylistReceiver, runner::PlaylistRunner};

// Imports
use {
	crossbeam::channel,
	parking_lot::RwLock,
	std::{collections::HashSet, path::PathBuf, sync::Arc},
};

/// A playlist image
#[derive(PartialEq, Eq, Clone, Hash, Debug)]
pub enum PlaylistImage {
	/// File path
	File(PathBuf),
	// TODO: URL
}

/// Playlist event
enum Event {
	/// Remove Image
	RemoveImg(Arc<PlaylistImage>),

	/// Change root path
	ChangeRoot(PathBuf),
}

/// Playlist inner
#[derive(Debug)]
struct PlaylistInner {
	/// Root path
	root_path: Option<PathBuf>,

	/// All images
	all_images: HashSet<Arc<PlaylistImage>>,

	/// Current images
	cur_images: Vec<Arc<PlaylistImage>>,
}

/// Creates the playlist service
#[must_use]
pub fn create() -> (PlaylistRunner, PlaylistReceiver, PlaylistManager) {
	// Create the channels
	// Note: Since processing a single playlist item is cheap, we
	//       use a small buffer size
	let (img_tx, img_rx) = channel::bounded(1);
	let (event_tx, event_rx) = channel::unbounded();

	let inner = PlaylistInner {
		root_path:  None,
		all_images: HashSet::new(),
		cur_images: vec![],
	};
	let inner = Arc::new(RwLock::new(inner));

	let playlist_runner = PlaylistRunner {
		inner: Arc::clone(&inner),
		img_tx,
		img_rx: img_rx.clone(),
		event_rx,
	};
	let playlist_receiver = PlaylistReceiver {
		img_rx,
		event_tx: event_tx.clone(),
	};
	let playlist_manager = PlaylistManager { inner, event_tx };

	(playlist_runner, playlist_receiver, playlist_manager)
}
