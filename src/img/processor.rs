//! Image processing

// Modules
mod process;

// Imports
use super::{ImageRequest, LoadedImageReceiver};
use crate::{
	sync::{once_channel, priority_spmc},
	util, ImageLoader, ProcessedImage,
};
use anyhow::Context;
use std::{num::NonZeroUsize, thread};

/// Image processor
pub struct ImageProcessor {
	/// Request sender
	request_sender: priority_spmc::Sender<(ImageRequest, once_channel::Sender<ProcessedImage>)>,
}

impl ImageProcessor {
	/// Creates a new image processor
	///
	/// # Errors
	/// Returns an error if unable to spawn all processor threads
	pub fn new(
		image_loader: &ImageLoader, processor_threads: Option<usize>, upscale: bool, downscale: bool,
		upscale_waifu2x: bool,
	) -> Result<Self, anyhow::Error> {
		// Start the image loader threads
		// Note: Requests shouldn't be limited,
		// TODO: Find a better way to do a priority based two-way communication channel.
		let (request_sender, request_receiver) = priority_spmc::channel(None);
		let processor_threads = processor_threads.unwrap_or_else(default_process_threads).max(1);
		for thread_idx in 0..processor_threads {
			let request_receiver = request_receiver.clone();
			let raw_image_receiver = image_loader.receiver();
			thread::Builder::new()
				.name(format!("Image processor #{thread_idx}"))
				.spawn(move || {
					match self::image_processor(
						&request_receiver,
						&raw_image_receiver,
						upscale,
						downscale,
						upscale_waifu2x,
					) {
						Ok(()) => log::debug!("Image processor #{thread_idx} successfully quit"),
						Err(err) => log::warn!("Image processor #{thread_idx} returned `Err`: {err:?}"),
					}
				})
				.context("Unable to spawn image processor")?;
		}

		Ok(Self { request_sender })
	}

	/// Requests an image
	///
	/// # Errors
	/// Returns an error if unable to send a request
	pub fn request(&self, request: ImageRequest, priority: usize) -> Result<ProcessedImageReceiver, anyhow::Error> {
		// Create the channel and send the request
		let (image_sender, image_receiver) = once_channel::channel();
		self.request_sender
			.send((request, image_sender), priority)
			.context("Unable to send request to processor thread")?;

		Ok(ProcessedImageReceiver { image_receiver })
	}
}


/// Processed image receiver
#[derive(Debug)]
pub struct ProcessedImageReceiver {
	/// Image receiver
	image_receiver: once_channel::Receiver<ProcessedImage>,
}

impl ProcessedImageReceiver {
	/// Receives the image, waiting if not ready yet
	pub fn recv(self) -> Result<ProcessedImage, anyhow::Error> {
		self.image_receiver
			.recv()
			.context("Unable to get image from processor thread")
	}

	/// Attempts to receive the image, returning `Ok(Err)` if not ready yet
	pub fn try_recv(mut self) -> Result<Result<ProcessedImage, Self>, anyhow::Error> {
		// Try to get the result
		match self.image_receiver.try_recv() {
			Ok(image) => Ok(Ok(image)),
			Err(once_channel::TryRecvError::NotReady(receiver)) => {
				self.image_receiver = receiver;
				Ok(Err(self))
			},
			Err(_) => anyhow::bail!("Unable to get image from processor thread"),
		}
	}
}


/// Returns the default number of process threads to use
fn default_process_threads() -> usize {
	thread::available_parallelism().map_or(1, NonZeroUsize::get)
}

/// Image processing thread function
///
/// Responsible for receiving requests and processing images for them.
fn image_processor(
	request_receiver: &priority_spmc::Receiver<(ImageRequest, once_channel::Sender<ProcessedImage>)>,
	raw_image_receiver: &LoadedImageReceiver, upscale: bool, downscale: bool, upscale_waifu2x: bool,
) -> Result<(), anyhow::Error> {
	loop {
		// Get the next request
		let (request, sender) = match request_receiver.recv() {
			Ok(value) => value,
			Err(_) => return Ok(()),
		};

		// Then try to get images until we send ones
		'get_img: loop {
			// Then get the image
			let (path, image) = match raw_image_receiver.recv() {
				Ok(value) => value,
				Err(_) => return Ok(()),
			};

			// And try to process it
			// Note: We can ignore errors on sending, since other senders might still be alive
			#[allow(clippy::let_underscore_drop)]
			match util::measure(|| process::process_image(&path, image, request, upscale, downscale, upscale_waifu2x)) {
				// If we got it, send it
				(Ok(image), duration) => {
					log::trace!("Took {duration:?} to process {path:?}");
					let _ = sender.send(image);
					break 'get_img;
				},
				// If we didn't manage to, log and try again with another path
				(Err(err), _) => log::info!("Unable to process {path:?}: {err:?}"),
			};
		}
	}
}
