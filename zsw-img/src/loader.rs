//! Image loader
//!
//! See the [`ImageLoader`] type for more details on how image loading
//! works.

// Modules
mod load;

// Imports
use {
	super::Image,
	zsw_playlist::{PlaylistImage, PlaylistReceiver},
};

/// Image loader service
#[derive(Debug)]
pub struct ImageLoaderService {
	/// Image sender
	image_tx: async_channel::Sender<Image>,

	/// Image receiver
	image_rx: async_channel::Receiver<Image>,
}

impl ImageLoaderService {
	/// Creates a new image loader.
	#[must_use]
	pub fn new() -> Self {
		// Note: We have the lowest possible bound due to images being quite big
		// TODO: Make this customizable and even be able to be 0?
		let (image_tx, image_rx) = async_channel::bounded(1);

		Self { image_tx, image_rx }
	}

	/// Runs this image loader
	///
	/// Multiple image loaders may run at the same time
	pub async fn run(&self, playlist_receiver: PlaylistReceiver) {
		loop {
			// Get the next image, or quit if no more
			let Some(image) = playlist_receiver.next() else {
				break;
			};

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
						tracing::info!(?path, ?err, "Unable to load file");
						playlist_receiver.remove_image(image);
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

impl Default for ImageLoaderService {
	fn default() -> Self {
		Self::new()
	}
}
