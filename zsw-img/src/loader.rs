//! Image loader
//!
//! See the [`ImageLoader`] type for more details on how image loading
//! works.

// Modules
mod load;

// Imports
use {
	super::Image,
	zsw_playlist::{Playlist, PlaylistImage},
	zsw_side_effect_macros::side_effect,
	zsw_util::MightLock,
};

/// Image loader
#[derive(Debug)]
pub struct ImageLoader {
	/// Image sender
	image_tx: async_channel::Sender<Image>,

	/// Image receiver
	image_rx: async_channel::Receiver<Image>,
}

impl ImageLoader {
	/// Creates a new image loader.
	#[must_use]
	pub fn new() -> Self {
		let (image_tx, image_rx) = async_channel::bounded(1);

		Self { image_tx, image_rx }
	}

	/// Runs this image loader
	///
	/// Multiple image loaders may run at the same time
	///
	/// # Locking
	/// Locks the `zsw_playlist::PlaylistLock` lock on `playlist`
	#[side_effect(MightLock<zsw_playlist::PlaylistLock<'_>>)]
	pub async fn run(&self, playlist: &Playlist) -> ! {
		loop {
			// DEADLOCK: Caller ensures we can lock it
			let image = playlist.next().await.allow::<MightLock<zsw_playlist::PlaylistLock>>();

			match &*image {
				PlaylistImage::File(path) => match load::load_image(path) {
					// If we got it, send it
					Ok(image) => {
						let image = Image {
							path: path.clone(),
							image,
						};

						// Note: This can't return an `Err` because `self` owns a receiver
						// DEADLOCK: We don't hold any lock while sending
						self.image_tx.send(image).await.expect("Image receiver was closed");
					},

					// If we couldn't load, log, remove the path and retry
					Err(err) => {
						log::info!("Unable to load {path:?}: {err:?}");

						// DEADLOCK: Caller ensures we can lock it
						let mut playlist_lock = playlist
							.lock_inner()
							.await
							.allow::<MightLock<zsw_playlist::PlaylistLock>>();
						playlist.remove_image(&mut playlist_lock, &image).await;
					},
				},
			}
		}
	}

	/// Attempts to receive the image
	#[must_use]
	#[allow(clippy::missing_panics_doc)] // It's an internal assertion
	pub fn try_recv(&self) -> Option<Image> {
		// Try to get the result
		// Note: This can't return an `Err` because `self` owns a sender
		match self.image_rx.try_recv() {
			Ok(image) => Some(image),
			Err(async_channel::TryRecvError::Empty) => None,
			Err(async_channel::TryRecvError::Closed) => panic!("Image loader sender was dropped"),
		}
	}
}

impl Default for ImageLoader {
	fn default() -> Self {
		Self::new()
	}
}
