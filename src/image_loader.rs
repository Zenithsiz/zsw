//! Image loader
//!
//! See the [`ImageLoader`] type for more details on how image loading
//! works.

// Modules
mod load;
mod request;

// Imports
use self::request::ImageRequest;
use crate::{
	path_loader::PathReceiver,
	sync::{once_channel, priority_spmc},
	PathLoader,
};
use anyhow::Context;
use cgmath::Vector2;
use image::Rgba;
use std::{num::NonZeroUsize, ops::Deref, thread};

/// Image buffer
pub type ImageBuffer = image::ImageBuffer<Rgba<u8>, Vec<u8>>;

/// Responder for an image loader
type ImageResponder = once_channel::Sender<Result<ImageBuffer, ()>>;

/// Image loader
pub struct ImageLoader {
	/// Image request sender
	image_request_sender: priority_spmc::Sender<(ImageRequest, ImageResponder)>,
}

impl ImageLoader {
	/// Creates a new image loader
	/// 
	/// # Errors
	/// Returns an error if unable to create all the loader threads
	pub fn new(
		path_loader: &PathLoader, loader_threads: Option<usize>, upscale_waifu2x: bool,
	) -> Result<Self, anyhow::Error> {
		// Start the image loader distribution thread
		let (image_request_sender, image_request_receiver) = priority_spmc::channel();
		let loader_threads = loader_threads.unwrap_or_else(default_loader_threads).max(1);
		for thread_idx in 0..loader_threads {
			let request_receiver = image_request_receiver.clone();
			let path_receiver = path_loader.receiver();
			thread::Builder::new()
				.name(format!("Image loader #{thread_idx}"))
				.spawn(
					move || match self::image_loader(&request_receiver, &path_receiver, upscale_waifu2x) {
						Ok(()) => log::debug!("Image loader #{thread_idx} successfully quit"),
						Err(err) => log::warn!("Image loader #{thread_idx} returned `Err`: {err:?}"),
					},
				)
				.context("Unable to spawn image loader")?;
		}


		Ok(Self { image_request_sender })
	}

	/// Queues an image to be loaded for a certain window size
	///
	/// # Errors
	/// Returns an error if unable to get a path to queue up, or if unable to
	/// send a request to a loader thread.
	pub fn queue(&self, window_size: Vector2<u32>, priority: usize) -> Result<ImageReceiver, anyhow::Error> {
		// Send a request
		let (image_sender, image_receiver) = once_channel::channel();
		self.image_request_sender
			.send((ImageRequest { window_size }, image_sender), priority)
			.context("Unable to send request to image loader")?;

		Ok(ImageReceiver {
			window_size,
			image_receiver,
		})
	}
}

/// An image receiver
#[derive(Debug)]
pub struct ImageReceiver {
	/// Window size
	window_size: Vector2<u32>,

	/// Image requester
	image_receiver: once_channel::Receiver<Result<ImageBuffer, ()>>,
}

impl ImageReceiver {
	/// Receives the image, waiting if not ready yet
	pub fn recv(self, image_loader: &ImageLoader, retry_priority: usize) -> Result<ImageBuffer, anyhow::Error> {
		// Get the result
		let res = self
			.image_receiver
			.recv()
			.context("Unable to get image from loader thread")?;

		// And check if we got the image
		match res {
			// If we did, return it
			Ok(image) => Ok(image),

			// Else retry
			Err(()) => image_loader
				.queue(self.window_size, retry_priority)
				.context("Unable to re-queue image")?
				.recv(image_loader, retry_priority),
		}
	}

	/// Attempts to receive the image, returning `Ok(Err)` if not ready yet
	pub fn try_recv(
		mut self, image_loader: &ImageLoader, retry_priority: usize,
	) -> Result<Result<ImageBuffer, Self>, anyhow::Error> {
		// Try to get the result
		let res = match self.image_receiver.try_recv() {
			Ok(res) => res,
			Err(once_channel::TryRecvError::NotReady(receiver)) => {
				self.image_receiver = receiver;
				return Ok(Err(self));
			},
			Err(_) => anyhow::bail!("Unable to get image from loader thread"),
		};

		// And check if we got the image
		match res {
			// If we did, return it
			Ok(image) => Ok(Ok(image)),

			// Else requeue and return
			Err(()) => Ok(Err(image_loader
				.queue(self.window_size, retry_priority)
				.context("Unable to re-queue image")?)),
		}
	}
}

/// Returns the default number of loader threads to use
fn default_loader_threads() -> usize {
	thread::available_parallelism().map_or(1, NonZeroUsize::get)
}

/// Image loader thread function
///
/// Responsible for receiving requests and loading them.
fn image_loader(
	request_receiver: &priority_spmc::Receiver<(ImageRequest, ImageResponder)>, path_receiver: &PathReceiver,
	upscale_waifu2x: bool,
) -> Result<(), anyhow::Error> {
	loop {
		// Get the path
		let path = match path_receiver.recv() {
			Ok(path) => path,
			Err(_) => return Ok(()),
		};

		// Try to load the image
		let image = match load::load_image(&path) {
			Ok(image) => {
				log::debug!("Finished loading {path:?}");
				image
			},
			// If we didn't manage to, remove the path and retry
			Err(err) => {
				log::info!("Unable to load {path:?}: {err:?}");
				let _ = path_receiver.remove_path(path.deref().clone());
				continue;
			},
		};

		// Get the next request
		let (request, sender) = match request_receiver.recv() {
			Ok(value) => value,
			Err(_) => return Ok(()),
		};

		// Then try to load it
		// Note: We can ignore errors on sending, since other senders might still be alive
		#[allow(clippy::let_underscore_drop)]
		match load::process_image(&path, image, &request, upscale_waifu2x) {
			// If we got it, send it
			Ok(image) => {
				log::debug!("Finished processing {path:?}");
				let _ = sender.send(Ok(image));
			},
			// If we didn't manage to, remove the path and retry
			Err(err) => {
				log::info!("Unable to process {path:?}: {err:?}");
				let _ = sender.send(Err(()));
				let _ = path_receiver.remove_path(path.deref().clone());
			},
		};
	}
}
