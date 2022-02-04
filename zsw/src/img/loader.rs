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
		paths,
		util::{self, extse::CrossBeamChannelReceiverSE, MightDeadlock},
	},
	anyhow::Context,
	zsw_side_effect_macros::side_effect,
};

/// Image loader
#[derive(Clone, Debug)]
pub struct ImageLoader {
	/// Image sender
	image_tx: crossbeam::channel::Sender<Image>,

	/// Paths receiver
	paths_rx: paths::Receiver,
}

impl ImageLoader {
	/// Runs this image loader
	///
	/// Multiple image loaders may run at the same time
	///
	/// # Deadlock
	/// Deadlocks if the path distributer deadlocks in [`paths::Distributer::run`],
	/// or if all receivers' deadlock in [`ImageReceiver::recv`].
	#[side_effect(MightDeadlock)]
	pub fn run(self) -> Result<(), anyhow::Error> {
		// DEADLOCK: Caller ensures the paths distributer doesn't deadlock
		while let Ok(path) = self.paths_rx.recv().allow::<MightDeadlock>() {
			match util::measure(|| load::load_image(&path)) {
				// If we got it, send it
				(Ok(image), duration) => {
					let format = util::image_format(&image);
					log::debug!(target: "zsw::perf", "Took {duration:?} to load {path:?} (format: {format})");

					let image = Image { path, image };
					if self.image_tx.send(image).is_err() {
						log::info!("No more receivers found, quitting");
						break;
					}
				},
				// If we couldn't load, log, remove the path and retry
				(Err(err), _) => {
					log::info!("Unable to load {path:?}: {err:?}");
					self.paths_rx.remove(&path);
				},
			};
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
	/// # Deadlock
	/// Deadlocks if the image loader deadlocks in [`ImageLoader::run`]
	#[side_effect(MightDeadlock)]
	pub fn recv(&self) -> Result<Image, anyhow::Error> {
		// DEADLOCK: Caller ensures we don't deadlock
		self.image_rx
			.recv_se()
			.allow::<MightDeadlock>()
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
pub fn new(paths_rx: paths::Receiver) -> (ImageLoader, ImageReceiver) {
	// TODO: Check if a 0 capacity channel is fine here.
	//       Given we'll have a few runner threads, each one
	//       will hold an image, which should be fine, but we might
	//       want to hold more? Maybe let the user decide somewhere.
	let (image_tx, image_rx) = crossbeam::channel::bounded(0);

	(ImageLoader { image_tx, paths_rx }, ImageReceiver { image_rx })
}
