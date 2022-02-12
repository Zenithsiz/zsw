//! Image loader
//!
//! See the [`ImageLoader`] type for more details on how image loading
//! works.

// Modules
mod load;

// Imports
use {
	super::Image,
	crate::{
		util::{
			extse::{CrossBeamChannelReceiverSE, CrossBeamChannelSenderSE},
			MightBlock,
		},
		Playlist,
		PlaylistImage,
	},
	zsw_side_effect_macros::side_effect,
};

/// Image loader
#[derive(Debug)]
pub struct ImageLoader {
	/// Image sender
	image_tx: crossbeam::channel::Sender<Image>,

	/// Image receiver
	image_rx: crossbeam::channel::Receiver<Image>,

	/// Closing sender
	close_tx: crossbeam::channel::Sender<()>,

	/// Closing receiver
	close_rx: crossbeam::channel::Receiver<()>,
}

impl ImageLoader {
	/// Creates a new image loader.
	#[must_use]
	pub fn new() -> Self {
		// Note: Making the close channel unbounded is what allows us to not block
		//       in `Self::stop`.
		let (image_tx, image_rx) = crossbeam::channel::bounded(0);
		let (close_tx, close_rx) = crossbeam::channel::unbounded();

		Self {
			image_tx,
			image_rx,
			close_tx,
			close_rx,
		}
	}

	/// Runs this image loader
	///
	/// Multiple image loaders may run at the same time
	///
	/// # Blocking
	/// Blocks until [`Playlist::run`] starts.
	/// Will block in it's own event loop until [`Self::close`] is called.
	#[allow(clippy::useless_transmute)] // `crossbeam::select` does it
	#[side_effect(MightBlock)]
	pub fn run(&self, playlist: &Playlist) -> Result<(), anyhow::Error> {
		loop {
			// DEADLOCK: Caller guarantees that `Playlist::run` will running
			let image = playlist.next().allow::<MightBlock>();
			match &*image {
				PlaylistImage::File(path) => match load::load_image(path) {
					// If we got it, send it
					Ok(image) => {
						let image = Image {
							path: path.clone(),
							image,
						};

						// DEADLOCK: Caller can call `Self::stop` for us to stop at any moment.
						crossbeam::select! {
							// Try to send an image
							// Note: This can't return an `Err` because `self` owns a receiver
							send(self.image_tx, image) -> res => res.expect("Image receiver was closed"),

							// If we get anything in the close channel, break
							// Note: This can't return an `Err` because `self` owns a receiver
							recv(self.close_rx) -> res => {
								res.expect("On-close sender was closed");
								break
							},
						}
					},

					// If we couldn't load, log, remove the path and retry
					Err(err) => {
						log::info!("Unable to load {path:?}: {err:?}");
						playlist.remove_image(&image);
					},
				},
			}
		}

		Ok(())
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

	/// Receives the image, waiting if not ready yet
	///
	/// # Blocking
	/// Blocks until [`Self::run`] starts running.
	#[side_effect(MightBlock)]
	pub fn recv(&self) -> Image {
		// Note: This can't return an `Err` because `self` owns a sender
		// DEADLOCK: Caller ensures `Self::run` will eventually run.
		self.image_rx
			.recv_se()
			.allow::<MightBlock>()
			.expect("Image sender was closed")
	}

	/// Attempts to receive the image
	pub fn try_recv(&self) -> Result<Option<Image>, anyhow::Error> {
		// Try to get the result
		match self.image_rx.try_recv() {
			Ok(image) => Ok(Some(image)),
			Err(crossbeam::channel::TryRecvError::Empty) => Ok(None),
			Err(_) => anyhow::bail!("Unable to get image from loader thread"),
		}
	}
}

impl Default for ImageLoader {
	fn default() -> Self {
		Self::new()
	}
}
