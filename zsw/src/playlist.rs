//! Image playlist
//!
//! Manages the paths/urls of all images to display.

// Imports
use {
	crate::util::{
		extse::{CrossBeamChannelReceiverSE, CrossBeamChannelSenderSE, ParkingLotMutexSe},
		MightBlock,
	},
	parking_lot::Mutex,
	rand::prelude::SliceRandom,
	std::{
		collections::HashSet,
		path::{Path, PathBuf},
		sync::Arc,
	},
	zsw_side_effect_macros::side_effect,
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

	/// Closing sender
	close_tx: crossbeam::channel::Sender<()>,

	/// Closing receiver
	close_rx: crossbeam::channel::Receiver<()>,

	/// Inner
	inner: Mutex<Inner>,
}

impl Playlist {
	/// Creates a new, empty, playlist
	#[must_use]
	pub fn new() -> Self {
		// Note: Making the close channel unbounded is what allows us to not block
		//       in `Self::stop`.
		let (img_tx, img_rx) = crossbeam::channel::bounded(0);
		let (close_tx, close_rx) = crossbeam::channel::unbounded();

		// Create the empty inner data
		let inner = Inner { images: HashSet::new() };

		Self {
			img_tx,
			img_rx,
			close_tx,
			close_rx,
			inner: Mutex::new(inner),
		}
	}

	/// Runs the playlist
	///
	/// # Blocking
	/// Will block in it's own event loop until [`Self::stop`] is called.
	#[allow(clippy::useless_transmute)] // `crossbeam::select` does it
	pub fn run(&self) {
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

			// Then try to send each one and check for the close channel
			for image in images.drain(..) {
				crossbeam::select! {
					// Try to send an image
					// Note: This can't return an `Err` because `self` owns a receiver
					send(self.img_tx, image) -> res => res.expect("Image receiver was closed"),

					// If we get anything in the close channel, break
					// Note: This can't return an `Err` because `self` owns a receiver
					recv(self.close_rx) -> res => {
						res.expect("On-close sender was closed");
						break
					},
				}
			}
		}
	}

	/// Stops the `run` loop
	pub fn stop(&self) {
		// Note: This can't return an `Err` because `self` owns a sender
		// DEADLOCK: The channel is unbounded, so this will not block.
		self.close_tx
			.send_se(())
			.allow::<MightBlock>()
			.expect("On-close receiver was closed");
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
	///
	/// # Blocking
	/// Blocks until [`Self::run`] starts running.
	#[side_effect(MightBlock)]
	pub fn next(&self) -> Arc<PlaylistImage> {
		// Note: This can't return an `Err` because `self` owns a sender
		// DEADLOCK: Caller ensures `Self::run` will eventually run.
		self.img_rx
			.recv_se()
			.allow::<MightBlock>()
			.expect("Image sender was closed")
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
