//! Image playlist
//!
//! Manages the paths/urls of all images to display.

// Imports
use {
	crate::util,
	async_lock::Mutex,
	rand::prelude::SliceRandom,
	std::{collections::HashSet, path::PathBuf, sync::Arc},
};

/// Inner
#[derive(Clone, Debug)]
struct Inner {
	/// Root path
	// TODO: Use this properly
	root_path: Option<PathBuf>,

	/// All images
	images: HashSet<Arc<PlaylistImage>>,

	/// Current images
	cur_images: Vec<Arc<PlaylistImage>>,
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
		let inner = Inner {
			root_path:  None,
			images:     HashSet::new(),
			cur_images: vec![],
		};

		Self {
			img_tx,
			img_rx,
			inner: Mutex::new(inner),
		}
	}

	/// Runs the playlist
	pub async fn run(&self) -> ! {
		loop {
			// Get the next image to send
			// DEADLOCK: We ensure we don't block while `inner` is locked
			// Note: It's important to not have this in the match expression, as it would
			//       keep the lock through the whole match.
			let next = self.inner.lock().await.cur_images.pop();

			// Then check if we got it
			match next {
				// If we got it, send it
				// Note: This can't return an `Err` because `self` owns a receiver
				Some(image) => self.img_tx.send(image).await.expect("Image receiver was closed"),

				// Else get the next batch and shuffle them
				// DEADLOCK: We ensure we don't block while `inner` is locked
				None => {
					let mut inner = self.inner.lock().await;
					let inner = &mut *inner;
					inner.cur_images.extend(inner.images.iter().cloned());
					inner.cur_images.shuffle(&mut rand::thread_rng());
				},
			}
		}
	}

	/// Removes an image
	pub async fn remove_image(&self, image: &PlaylistImage) {
		// DEADLOCK: We ensure we don't block while `inner` is locked
		let mut inner = self.inner.lock().await;

		// Note: We don't care if the image actually existed or not
		let _ = inner.images.remove(image);
	}

	/// Sets the root path
	pub async fn set_root_path(&self, root_path: PathBuf) {
		// DEADLOCK: We ensure we don't block while `inner` is locked
		let mut inner = self.inner.lock().await;

		// Remove all existing paths and add new ones
		inner.images.clear();
		for path in util::dir_files_iter(root_path.clone()) {
			let _ = inner.images.insert(Arc::new(PlaylistImage::File(path)));
		}

		// Remove all current paths too
		inner.cur_images.clear();

		// Save the root path
		inner.root_path = Some(root_path);
	}

	/// Returns the root path
	pub async fn root_path(&self) -> Option<PathBuf> {
		self.inner.lock().await.root_path.clone()
	}

	/// Retrieves the next image
	pub async fn next(&self) -> Arc<PlaylistImage> {
		// Note: This can't return an `Err` because `self` owns a sender
		self.img_rx.recv().await.expect("Image sender was closed")
	}

	/// Peeks the next images
	///
	/// # Blocking
	/// Deadlocks if `f` blocks.
	pub async fn peek_next(&self, mut f: impl FnMut(&PlaylistImage) + Send) {
		// DEADLOCK: We ensure we don't block while `inner` is locked.
		//           Caller ensures `f` doesn't block
		let inner = self.inner.lock().await;

		for image in inner.cur_images.iter().rev() {
			f(image);
		}
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
