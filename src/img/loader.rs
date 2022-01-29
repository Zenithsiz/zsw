//! Image loader
//!
//! See the [`ImageLoader`] type for more details on how image loading
//! works.

// Modules
mod load;

// Imports
use super::Image;
use crate::{paths, util};
use anyhow::Context;

/// Image loader
#[derive(Debug)]
pub struct ImageLoader {
	/// Image receiver
	image_rx: crossbeam::channel::Receiver<Image>,

	/// Image sender
	image_tx: crossbeam::channel::Sender<Image>,

	/// Paths receiver
	paths_rx: paths::Receiver,
}

impl ImageLoader {
	/// Creates a new image loader
	///
	/// # Errors
	/// Returns an error if unable to create all the loader threads
	pub fn new(paths_rx: paths::Receiver) -> Result<Self, anyhow::Error> {
		// Create the image channel with the number of threads, since that's likely to
		// be the number of runners we have
		let loader_threads = std::thread::available_parallelism()
			.context("Unable to get available parallelism")?
			.get();
		let (image_tx, image_rx) = crossbeam::channel::bounded(2 * loader_threads);

		Ok(Self {
			image_rx,
			image_tx,
			paths_rx,
		})
	}

	/// Runs an image loader.
	///
	/// Multiple image loaders may run at the same time
	pub fn run(&self) -> Result<(), anyhow::Error> {
		self::run_image_loader(&self.image_tx, &self.paths_rx)
	}

	/// Receives the image, waiting if not ready yet
	pub fn recv(&self) -> Result<Image, anyhow::Error> {
		self.image_rx.recv().context("Unable to get image from loader thread")
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

/// Runs the image loader
fn run_image_loader(
	image_tx: &crossbeam::channel::Sender<Image>, paths_rx: &paths::Receiver,
) -> Result<(), anyhow::Error> {
	while let Ok(path) = paths_rx.recv() {
		match util::measure(|| load::load_image(&path)) {
			// If we got it, send it
			(Ok(image), duration) => {
				log::trace!(target: "zsw::perf", "Took {duration:?} to load {path:?}");
				if image_tx.send(image.to_rgba8()).is_err() {
					log::info!("No more receivers found, quitting");
					break;
				}
			},
			// If we didn't manage to, log and try again with another path
			(Err(err), _) => {
				log::info!("Unable to load {path:?}: {err:?}");
				paths_rx.remove(&path);
			},
		};
	}

	Ok(())
}
