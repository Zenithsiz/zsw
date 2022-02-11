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
			self,
			extse::{CrossBeamChannelReceiverSE, CrossBeamChannelSenderSE},
			MightBlock,
		},
		Playlist,
		PlaylistImage,
	},
	anyhow::Context,
	zsw_side_effect_macros::side_effect,
};

/// Image loader
#[derive(Clone, Debug)]
pub struct ImageLoader {
	/// Image sender
	image_tx: crossbeam::channel::Sender<Image>,
}

impl ImageLoader {
	/// Runs this image loader
	///
	/// Multiple image loaders may run at the same time
	///
	/// # Blocking
	/// Blocks until the path distributer sends a path via [`Distributer::run`](super::Distributer::run).
	/// Blocks until a receiver receives via [`ImageReceiverReceiver::recv`](`ImageReceiver::recv`).
	#[side_effect(MightBlock)]
	pub fn run(self, playlist: &Playlist) -> Result<(), anyhow::Error> {
		loop {
			let image = playlist.next();
			match &*image {
				PlaylistImage::File(path) => {
					match util::measure(|| load::load_image(path)) {
						// If we got it, send it
						(Ok(image), duration) => {
							let format = util::image_format(&image);
							log::debug!(target: "zsw::perf", "Took {duration:?} to load {path:?} (format: {format})");

							// DEADLOCK: Caller is responsible for avoiding deadlocks
							let image = Image {
								path: path.clone(),
								image,
							};
							if self.image_tx.send_se(image).allow::<MightBlock>().is_err() {
								log::info!("No more receivers found, quitting");
								break;
							}
						},
						// If we couldn't load, log, remove the path and retry
						(Err(err), _) => {
							log::info!("Unable to load {path:?}: {err:?}");
							playlist.remove_image(&image);
						},
					};
				},
			}
		}

		Ok(())
	}
}

/// Image receiver
#[derive(Clone, Debug)]
pub struct ImageReceiver {
	/// Image receiver
	image_rx: crossbeam::channel::Receiver<Image>,
}

impl ImageReceiver {
	/// Receives the image, waiting if not ready yet
	///
	/// # Blocking
	/// Blocks until the loader sends an image via [`ImageLoader::run`]
	#[side_effect(MightBlock)]
	pub fn recv(&self) -> Result<Image, anyhow::Error> {
		// DEADLOCK: Caller is responsible for avoiding deadlocks
		self.image_rx
			.recv_se()
			.allow::<MightBlock>()
			.context("Unable to get image from loader thread")
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

/// Creates a new image loader.
pub fn new() -> (ImageLoader, ImageReceiver) {
	// TODO: Check if a 0 capacity channel is fine here.
	//       Given we'll have a few runner threads, each one
	//       will hold an image, which should be fine, but we might
	//       want to hold more? Maybe let the user decide somewhere.
	let (image_tx, image_rx) = crossbeam::channel::bounded(0);

	(ImageLoader { image_tx }, ImageReceiver { image_rx })
}
