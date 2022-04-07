//! Image loader
//!
//! See the [`ImageLoader`] type for more details on how image loading
//! works.

// Modules
mod load;

// Imports
use {
	super::Image,
	zsw_playlist::{Playlist, PlaylistImage, PlaylistResource},
	zsw_util::{Resources, Services},
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
	/// # Blocking
	/// Locks [`zsw_playlist::PlaylistLock`] on `playlist`
	pub async fn run<S, R>(&self, services: &S, resources: &R) -> !
	where
		S: Services<Playlist>,
		R: Resources<PlaylistResource>,
	{
		let playlist = services.service::<Playlist>();

		loop {
			// DEADLOCK: Caller ensures we can lock it
			let image = playlist.next().await;

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

						// DEADLOCK: Caller ensures we can lock it
						let mut playlist_resource = resources.resource::<PlaylistResource>().await;
						playlist.remove_image(&mut playlist_resource, &image).await;
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
