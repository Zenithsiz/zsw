//! Image playlist
//!
//! Manages the paths/urls of all images to display.

// Lints
#![allow(clippy::disallowed_methods)] // TODO:

// Imports
use {
	parking_lot::Mutex,
	rand::prelude::SliceRandom,
	std::{
		collections::HashSet,
		path::{Path, PathBuf},
		sync::Arc,
	},
};

/// Inner
#[derive(Clone, Debug)]
struct Inner {
	/// All images
	images: HashSet<Arc<PlaylistImage>>,
}

/// Image playlist
#[derive(Debug)]
pub struct Playlist {
	/// Image sender
	img_tx: crossbeam::channel::Sender<Arc<PlaylistImage>>,

	/// Image receiver
	img_rx: crossbeam::channel::Receiver<Arc<PlaylistImage>>,

	/// Inner
	inner: Mutex<Inner>,
}

impl Playlist {
	/// Creates a new, empty, playlist
	#[must_use]
	pub fn new() -> Self {
		// Create the image channel
		let (img_tx, img_rx) = crossbeam::channel::bounded(0);

		// Create the empty inner data
		let inner = Inner { images: HashSet::new() };

		Self {
			img_tx,
			img_rx,
			inner: Mutex::new(inner),
		}
	}

	/// Runs the playlist daemon
	///
	/// Will exit an event is sent to `close_channel`.
	#[allow(clippy::useless_transmute)] // `crossbeam::select` does it
	pub fn run(&self, close_rx: &crossbeam::channel::Receiver<()>) {
		// All images to send
		let mut images = vec![];

		loop {
			// Retrieve the next images and shuffle them
			images.extend(self.inner.lock().images.iter().cloned());
			images.shuffle(&mut rand::thread_rng());

			// Then try to send each one and check for the close channel
			for image in images.drain(..) {
				crossbeam::select! {
					// Try to send an image
					// Note: This can't return an `Err` because `self` owns a receiver
					send(self.img_tx, image) -> res => res.expect("Receiver was closed"),

					// If we get anything in the close channel, break
					recv(close_rx) -> res => {
						match res {
							Ok(()) => log::info!("Received close event, quitting"),
							Err(_) => log::warn!("Close channel was closed, quitting"),
						}
						break
					},
				}
			}
		}
	}

	/// Clears all existing images
	pub fn clear(&self) {
		self.inner.lock().images.clear();
	}

	/// Removes an image
	pub fn remove_image(&self, image: &PlaylistImage) {
		let mut inner = self.inner.lock();

		// Note: We don't care if the image actually existed or not
		let _ = inner.images.remove(image);
	}

	/// Adds all images from a directory.
	///
	/// # Errors
	/// Logs all errors via `log::warn`
	pub fn add_dir(&self, dir_path: &Path) {
		let mut inner = self.inner.lock();

		// Add all paths
		log::info!("Loading all paths from {dir_path:?}");
		let ((), duration) = crate::util::measure(|| {
			crate::util::visit_files_dir(dir_path, &mut |path| {
				let _ = inner.images.insert(Arc::new(PlaylistImage::File(path)));
				Ok::<(), !>(())
			})
			.into_ok();
		});
		log::trace!(target: "zsw::perf", "Took {duration:?} to load all paths from {dir_path:?}");
	}

	/// Retrieves the next image
	pub fn next(&self) -> Arc<PlaylistImage> {
		// Note: This can't return an `Err` because `self` owns a sender
		self.img_rx.recv().expect("Sender was closed")
	}
}

impl Default for Playlist {
	fn default() -> Self {
		Self::new()
	}
}

/// A playlist image
#[derive(PartialEq, Eq, Clone, Hash, Debug)]
pub enum PlaylistImage {
	/// File path
	File(PathBuf),
	// TODO: URL
}
