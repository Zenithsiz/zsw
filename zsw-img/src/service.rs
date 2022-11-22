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
#[derive(Clone, Debug)]
pub struct ImageLoader {
	/// Image sender
	image_tx: async_channel::Sender<Image>,
}

impl ImageLoader {
	/// Runs this image loader
	///
	/// Multiple image loaders may run at the same time
	pub async fn run(self, playlist_receiver: PlaylistReceiver) {
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
}

/// Image receiver
#[derive(Debug)]
pub struct ImageReceiver {
	/// Image receiver
	image_rx: async_channel::Receiver<Image>,
}


impl ImageReceiver {
	/// Attempts to receive the image
	#[must_use]
	pub fn try_recv(&self) -> Option<Image> {
		self.image_rx.try_recv().ok()
	}
}

/// Creates the image loader service
#[must_use]
pub fn create() -> (ImageLoader, ImageReceiver) {
	// Create the image channel
	// Note: We have the lowest possible bound due to images being quite big
	// TODO: Make this customizable and even be able to be 0?
	let (image_tx, image_rx) = async_channel::bounded(1);

	(ImageLoader { image_tx }, ImageReceiver { image_rx })
}
