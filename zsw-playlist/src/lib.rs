//! Image playlist
//!
//! Manages the paths/urls of all images to display.

// Features
#![feature(never_type)]

// Imports
use {
	crossbeam::channel,
	parking_lot::RwLock,
	rand::prelude::SliceRandom,
	std::{collections::HashSet, mem, path::PathBuf, sync::Arc},
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

/// Playlist receiver
///
/// Receives images from the playlist
#[derive(Clone, Debug)]
pub struct PlaylistReceiver {
	/// Image receiver
	img_rx: channel::Receiver<Arc<PlaylistImage>>,

	/// Event sender
	event_tx: channel::Sender<Event>,
}

impl PlaylistReceiver {
	/// Retrieves the next image.
	///
	/// Returns `None` if the playlist manager was closed
	#[must_use]
	pub fn next(&self) -> Option<Arc<PlaylistImage>> {
		self.img_rx.recv().ok()
	}

	/// Removes an image
	pub fn remove_image(&self, image: Arc<PlaylistImage>) {
		// TODO: Care about this?
		let _res = self.event_tx.send(Event::RemoveImg(image));
	}
}

/// Playlist manager
#[derive(Clone, Debug)]
pub struct PlaylistManager {
	/// Inner
	inner: Arc<RwLock<PlaylistInner>>,

	/// Event sender
	event_tx: channel::Sender<Event>,
}

impl PlaylistManager {
	/// Removes an image
	pub fn remove_image(&self, image: Arc<PlaylistImage>) {
		// TODO: Care about this?
		let _res = self.event_tx.send(Event::RemoveImg(image));
	}

	/// Sets the root path
	pub fn set_root_path(&self, root_path: impl Into<PathBuf>) {
		// TODO: Care about this?
		let _res = self.event_tx.send(Event::ChangeRoot(root_path.into()));
	}

	/// Returns the root path
	#[must_use]
	pub fn root_path(&self) -> Option<PathBuf> {
		self.inner.read().root_path.clone()
	}

	/// Returns the remaining images in the current shuffle
	#[must_use]
	pub fn peek_next(&self) -> Vec<Arc<PlaylistImage>> {
		self.inner.read().cur_images.clone()
	}
}

/// Playlist runner
///
/// Responsible for driving the
/// playlist receiver
#[derive(Debug)]
pub struct PlaylistRunner {
	/// Inner
	inner: Arc<RwLock<PlaylistInner>>,

	/// Image sender
	img_tx: channel::Sender<Arc<PlaylistImage>>,

	/// Event receiver
	event_rx: channel::Receiver<Event>,
}

impl PlaylistRunner {
	/// Runs the playlist runner.
	///
	/// Returns once the playlist receiver is dropped
	pub fn run(self) {
		'run: loop {
			// Lock the inner data for writing
			let mut inner = self.inner.write();

			match inner.all_images.is_empty() {
				// If we have no images, block on the next event
				true => {
					// Note: Important we drop this before blocking
					mem::drop(inner);

					let event = match self.event_rx.recv() {
						Ok(event) => event,
						Err(channel::RecvError) => break 'run,
					};
					let mut inner = self.inner.write();
					Self::handle_event(&mut inner, event);
					continue;
				},

				// Else just consume all events we have
				false => loop {
					let event = match self.event_rx.try_recv() {
						Ok(event) => event,
						Err(channel::TryRecvError::Empty) => break,
						Err(channel::TryRecvError::Disconnected) => break 'run,
					};
					Self::handle_event(&mut inner, event);
				},
			}

			// Check if we have a next image to send
			match inner.cur_images.pop() {
				// If we got it, send it.
				// Note: It's important to drop the lock while awaiting
				// Note: If the sender got closed in the meantime, quit
				Some(image) => {
					mem::drop(inner);
					if self.img_tx.send(image).is_err() {
						break;
					}
				},

				// Else get the next batch and shuffle them
				// Note: If we got here, `all_images` has at least 1 image,
				//       so we won't be "spin locking" here.
				None => {
					let inner = &mut *inner;

					tracing::debug!("Reshuffling playlist");
					inner.cur_images.extend(inner.all_images.iter().cloned());
					inner.cur_images.shuffle(&mut rand::thread_rng());
				},
			}
		}
	}

	/// Handles event `event`
	fn handle_event(inner: &mut PlaylistInner, event: Event) {
		match event {
			// Note: We don't care if the image actually existed or not
			Event::RemoveImg(image) => {
				let _ = inner.all_images.remove(&image);
			},

			Event::ChangeRoot(root_path) => {
				// Remove all existing paths and add new ones
				inner.all_images.clear();
				zsw_util::visit_dir(&root_path, &mut |path| {
					let _ = inner.all_images.insert(Arc::new(PlaylistImage::File(path)));
				});

				// Remove all current paths too
				inner.cur_images.clear();

				// Save the root path
				inner.root_path = Some(root_path);
			},
		};
	}
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
		event_rx,
	};
	let playlist_receiver = PlaylistReceiver {
		img_rx,
		event_tx: event_tx.clone(),
	};
	let playlist_manager = PlaylistManager { inner, event_tx };

	(playlist_runner, playlist_receiver, playlist_manager)
}
