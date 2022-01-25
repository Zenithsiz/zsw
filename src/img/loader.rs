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
use std::thread;

/// Image loader
#[derive(Debug)]
pub struct ImageLoader {
	/// Image receiver
	image_rx: crossbeam::channel::Receiver<Image>,
}

impl ImageLoader {
	/// Creates a new image loader
	///
	/// # Errors
	/// Returns an error if unable to create all the loader threads
	pub fn new(paths_rx: &paths::Receiver) -> Result<Self, anyhow::Error> {
		let loader_threads = std::thread::available_parallelism()
			.context("Unable to get available parallelism")?
			.get();

		// Start all the loader threads
		let (image_tx, image_rx) = crossbeam::channel::bounded(2 * loader_threads);
		for thread_idx in 0..loader_threads {
			let image_tx = image_tx.clone();
			let paths_rx = paths_rx.clone();
			let _loader_thread = thread::Builder::new()
				.name("Image loader".to_owned())
				.spawn(move || match self::image_loader(&image_tx, &paths_rx) {
					Ok(()) => log::debug!("Image loader #{thread_idx} successfully quit"),
					Err(err) => log::warn!("Image loader #{thread_idx} returned `Err`: {err:?}"),
				})
				.context("Unable to spawn image loader")?;
		}

		Ok(Self { image_rx })
	}

	/// Requests an image to be loaded
	///
	/// # Errors
	/// Returns an error if unable to send a request
	#[must_use]
	pub fn receiver(&self) -> ImageReceiver {
		ImageReceiver {
			image_rx: self.image_rx.clone(),
		}
	}

	/// Clears all images in the receiver
	pub fn clear(&self) {
		while self.image_rx.try_recv().is_ok() {}
	}
}

/// Image receiver
#[derive(Debug)]
pub struct ImageReceiver {
	/// Image receiver
	image_rx: crossbeam::channel::Receiver<Image>,
}

impl ImageReceiver {
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

/// Image loader thread function
fn image_loader(image_tx: &crossbeam::channel::Sender<Image>, paths_rx: &paths::Receiver) -> Result<(), anyhow::Error> {
	#[allow(clippy::while_let_loop)] // We might add more steps before/after getting a path
	loop {
		// Get the next path
		let path = match paths_rx.recv() {
			Ok(path) => path,
			Err(_) => break,
		};

		// And try to process it
		// Note: We can ignore errors on sending, since other senders might still be alive
		#[allow(clippy::let_underscore_drop)]
		match util::measure(|| load::load_image(&path)) {
			// If we got it, send it
			(Ok(image), duration) => {
				log::trace!("Took {duration:?} to load {path:?}");
				let _ = image_tx.send(image.to_rgba8());
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
