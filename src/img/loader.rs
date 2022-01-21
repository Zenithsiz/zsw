//! Image loader
//!
//! See the [`ImageLoader`] type for more details on how image loading
//! works.

// Modules
mod load;

// Imports
use super::{Image, ImageRequest};
use crate::{
	paths::{PathReceiver, Paths},
	sync::{once_channel, priority_spmc},
	util,
};
use anyhow::Context;
use std::{num::NonZeroUsize, thread};

/// Image loader
#[derive(Debug)]
pub struct ImageLoader {
	/// Request sender
	request_tx: priority_spmc::Sender<(ImageRequest, once_channel::Sender<Image>)>,
}

impl ImageLoader {
	/// Creates a new image loader
	///
	/// # Errors
	/// Returns an error if unable to create all the loader threads
	pub fn new(paths: &Paths) -> Result<Self, anyhow::Error> {
		// Start the image loader threads
		// Note: Requests shouldn't be limited,
		// TODO: Find a better way to do a priority based two-way communication channel.
		let (request_tx, request_rx) = priority_spmc::channel(None);
		let loader_threads = std::thread::available_parallelism()
			.context("Unable to get available parallelism")?
			.get();
		for thread_idx in 0..loader_threads {
			let request_rx = request_rx.clone();
			let path_rx = paths.receiver();
			let _loader_thread = thread::Builder::new()
				.name("Image loader".to_owned())
				.spawn(move || match self::image_loader(&request_rx, &path_rx) {
					Ok(()) => log::debug!("Image loader #{thread_idx} successfully quit"),
					Err(err) => log::warn!("Image loader #{thread_idx} returned `Err`: {err:?}"),
				})
				.context("Unable to spawn image loader")?;
		}

		Ok(Self { request_tx })
	}

	/// Requests an image to be loaded
	///
	/// # Errors
	/// Returns an error if unable to send a request
	pub fn request(&self, request: ImageRequest, priority: usize) -> Result<ImageReceiver, anyhow::Error> {
		// Create the channel and send the request
		let (image_tx, image_rx) = once_channel::channel();
		self.request_tx
			.send((request, image_tx), priority)
			.context("Unable to send request to loader thread")?;

		Ok(ImageReceiver { image_rx })
	}
}

/// Image loader arguments
#[derive(PartialEq, Clone, Copy, Debug)]
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)]
#[allow(clippy::struct_excessive_bools)] // It's a config
pub struct ImageLoaderArgs {
	/// Loader threads
	pub loader_threads: usize,

	/// If any upscaling should be done
	pub upscale: bool,

	/// If upscaling should be done with waifu 2x
	pub upscale_waifu2x: bool,

	/// If images can be loaded from the downscaled cache
	pub downscale_load_from_cache: bool,

	/// If images should be downscaled and saved to cache
	pub downscale_save_to_cache: bool,
}

impl Default for ImageLoaderArgs {
	fn default() -> Self {
		Self {
			loader_threads:            thread::available_parallelism().map_or(1, NonZeroUsize::get),
			upscale:                   false,
			upscale_waifu2x:           false,
			downscale_load_from_cache: true,
			downscale_save_to_cache:   false,
		}
	}
}


/// Image receiver
#[derive(Debug)]
pub struct ImageReceiver {
	/// Image receiver
	image_rx: once_channel::Receiver<Image>,
}

impl ImageReceiver {
	/// Receives the image, waiting if not ready yet
	pub fn recv(self) -> Result<Image, anyhow::Error> {
		self.image_rx.recv().context("Unable to get image from loader thread")
	}

	/// Attempts to receive the image, returning `Ok(Err(self))` if not ready yet
	///
	/// # Errors
	/// Returns `Err` if unable to get image from loader thread
	pub fn try_recv(mut self) -> Result<Result<Image, Self>, anyhow::Error> {
		// Try to get the result
		match self.image_rx.try_recv() {
			Ok(image) => Ok(Ok(image)),
			Err(once_channel::TryRecvError::NotReady(receiver)) => {
				self.image_rx = receiver;
				Ok(Err(self))
			},
			Err(_) => anyhow::bail!("Unable to get image from loader thread"),
		}
	}
}

/// Image loader thread function
fn image_loader(
	request_rx: &priority_spmc::Receiver<(ImageRequest, once_channel::Sender<Image>)>, path_rx: &PathReceiver,
) -> Result<(), anyhow::Error> {
	loop {
		// Get the next request
		let (request, sender) = match util::measure(|| request_rx.recv()) {
			(Ok(value), duration) => {
				log::trace!("Spent {duration:?} waiting for a request");
				value
			},
			(Err(_), _) => return Ok(()),
		};

		// Then try to get images until we send ones
		'get_img: loop {
			// Then get the image
			let path = match path_rx.recv() {
				Ok(path) => path,
				Err(_) => return Ok(()),
			};

			// And try to process it
			// Note: We can ignore errors on sending, since other senders might still be alive
			#[allow(clippy::let_underscore_drop)]
			match util::measure(|| load::load_image(&path, request)) {
				// If we got it, send it
				(Ok(image), duration) => {
					log::trace!("Took {duration:?} to load {path:?}");
					let _ = sender.send(image.to_rgba8());
					break 'get_img;
				},
				// If we didn't manage to, log and try again with another path
				(Err(err), _) => {
					log::info!("Unable to load {path:?}: {err:?}");
					let _ = path_rx.remove_path((*path).clone());
				},
			};
		}
	}
}
