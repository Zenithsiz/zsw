//! Image playlist
//!
//! Manages the paths/urls of all images to display.

// Imports
use {
	crate::util::{extse::ParkingLotMutexSe, MightBlock},
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
	img_tx: async_channel::Sender<Arc<PlaylistImage>>,

	/// Image receiver
	img_rx: async_channel::Receiver<Arc<PlaylistImage>>,

	/// Inner
	inner: Mutex<Inner>,
}

impl Playlist {
	/// Creates a new, empty, playlist
	#[must_use]
	pub fn new() -> Self {
		// Note: Making the close channel unbounded is what allows us to not block
		//       in `Self::stop`.
		let (img_tx, img_rx) = async_channel::bounded(1);

		// Create the empty inner data
		let inner = Inner { images: HashSet::new() };

		Self {
			img_tx,
			img_rx,
			inner: Mutex::new(inner),
		}
	}

	/// Runs the playlist
	#[allow(clippy::useless_transmute)] // `crossbeam::select` does it
	pub async fn run(&self) -> ! {
		// All images to send
		let mut images = vec![];

		loop {
			// Retrieve the next images and shuffle them
			// DEADLOCK: We ensure we don't block while `inner` is locked
			{
				let inner = self.inner.lock_se().allow::<MightBlock>();
				images.extend(inner.images.iter().cloned());
			}
			images.shuffle(&mut rand::thread_rng());

			// Then try to send each image
			for image in images.drain(..) {
				// Note: This can't return an `Err` because `self` owns a receiver
				self.img_tx.send(image).await.expect("Image receiver was closed");
			}
		}
	}

	/// Clears all existing images
	pub fn clear(&self) {
		// DEADLOCK: We ensure we don't block while `inner` is locked
		let mut inner = self.inner.lock_se().allow::<MightBlock>();
		inner.images.clear();
	}

	/// Removes an image
	pub fn remove_image(&self, image: &PlaylistImage) {
		// DEADLOCK: We ensure we don't block while `inner` is locked
		let mut inner = self.inner.lock_se().allow::<MightBlock>();

		// Note: We don't care if the image actually existed or not
		let _ = inner.images.remove(image);
	}

	/// Adds all images from a directory.
	///
	/// # Errors
	/// Logs all errors via `log::warn`
	pub fn add_dir(&self, dir_path: &Path) {
		// DEADLOCK: We ensure we don't block while `inner` is locked
		let mut inner = self.inner.lock_se().allow::<MightBlock>();

		// Add all paths
		log::info!("Loading all paths from {dir_path:?}");
		crate::util::visit_files_dir(dir_path, &mut |path| {
			let _ = inner.images.insert(Arc::new(PlaylistImage::File(path)));
			Ok::<(), !>(())
		})
		.into_ok();
	}

	/// Retrieves the next image
	pub async fn next(&self) -> Arc<PlaylistImage> {
		// Note: This can't return an `Err` because `self` owns a sender
		self.img_rx.recv().await.expect("Image sender was closed")
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
