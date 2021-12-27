//! Image loader
//!
//! See the [`ImageLoader`] type for more details on how image loading
//! works.

// Modules
mod load;

// Imports
use super::LoadedImage;
use crate::{path_loader::PathReceiver, util, PathLoader};
use anyhow::Context;
use crossbeam::channel as mpmc;
use std::{num::NonZeroUsize, ops::Deref, path::PathBuf, sync::Arc, thread};

/// Image loader
pub struct ImageLoader {
	/// Image receiver
	image_receiver: mpmc::Receiver<(Arc<PathBuf>, LoadedImage)>,
}

impl ImageLoader {
	/// Creates a new image loader
	///
	/// # Errors
	/// Returns an error if unable to create all the loader threads
	pub fn new(
		path_loader: &PathLoader, image_backlog: Option<usize>, loader_threads: Option<usize>,
	) -> Result<Self, anyhow::Error> {
		// Start the image loader threads
		let (image_sender, image_receiver) = mpmc::bounded(image_backlog.unwrap_or(1).max(1));
		let loader_threads = loader_threads.unwrap_or_else(default_loader_threads).max(1);
		for thread_idx in 0..loader_threads {
			let image_sender = image_sender.clone();
			let path_receiver = path_loader.receiver();
			thread::Builder::new()
				.name(format!("Image loader #{thread_idx}"))
				.spawn(move || match self::image_loader(&image_sender, &path_receiver) {
					Ok(()) => log::debug!("Image loader #{thread_idx} successfully quit"),
					Err(err) => log::warn!("Image loader #{thread_idx} returned `Err`: {err:?}"),
				})
				.context("Unable to spawn image loader")?;
		}


		Ok(Self { image_receiver })
	}

	/// Returns an image receiver
	#[must_use]
	pub fn receiver(&self) -> LoadedImageReceiver {
		LoadedImageReceiver {
			image_receiver: self.image_receiver.clone(),
		}
	}
}

/// Loaded image receiver
#[derive(Debug)]
pub struct LoadedImageReceiver {
	/// Image receiver
	image_receiver: mpmc::Receiver<(Arc<PathBuf>, LoadedImage)>,
}

impl LoadedImageReceiver {
	/// Receives the image, waiting if not ready yet
	pub fn recv(&self) -> Result<(Arc<PathBuf>, LoadedImage), anyhow::Error> {
		self.image_receiver
			.recv()
			.context("Unable to get image from loader thread")
	}

	/// Attempts to receive the image, returning `Ok(Err)` if not ready yet
	pub fn try_recv(&mut self) -> Result<Option<(Arc<PathBuf>, LoadedImage)>, anyhow::Error> {
		// Try to get the result
		match self.image_receiver.try_recv() {
			Ok(image) => Ok(Some(image)),
			Err(mpmc::TryRecvError::Empty) => Ok(None),
			Err(_) => anyhow::bail!("Unable to get image from loader thread"),
		}
	}
}

/// Returns the default number of loader threads to use
fn default_loader_threads() -> usize {
	thread::available_parallelism().map_or(1, NonZeroUsize::get)
}

/// Image loader thread function
fn image_loader(
	image_sender: &mpmc::Sender<(Arc<PathBuf>, LoadedImage)>, path_receiver: &PathReceiver,
) -> Result<(), anyhow::Error> {
	loop {
		// Get the path
		let path = match path_receiver.recv() {
			Ok(path) => path,
			Err(_) => return Ok(()),
		};

		// Try to load the image
		match util::measure(|| load::load_image(&path)) {
			// If we did, send it
			(Ok(image), duration) => {
				log::trace!("Took {duration:?} to load {path:?}");
				if image_sender.send((path, image)).is_err() {
					return Ok(());
				}
			},
			// If we didn't manage to, remove the path and retry
			(Err(err), _) => {
				log::warn!("Unable to load {path:?}: {err:?}");
				let _ = path_receiver.remove_path(path.deref().clone());
				continue;
			},
		}
	}
}
