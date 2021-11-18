//! Image loader
//!
//! See the [`ImageLoader`] type for more details on how image loading
//! works.

// Modules
mod load;
mod request;

// Imports
use self::request::{ImageRequest, LoadImageError};
use crate::sync::{once_channel, priority_spmc};
use anyhow::Context;
use cgmath::Vector2;
use image::Rgba;
use notify::Watcher;
use rand::prelude::SliceRandom;
use std::{
	mem,
	num::NonZeroUsize,
	ops::ControlFlow,
	path::{Path, PathBuf},
	sync::mpsc,
	thread,
	time::Duration,
};

/// Image buffer
pub type ImageBuffer = image::ImageBuffer<Rgba<u8>, Vec<u8>>;

/// Responder for the path distributer
type PathResponder = once_channel::Sender<Result<(), LoadImageError>>;

/// Responder for an image loader
type ImageResponder = once_channel::Sender<Result<ImageBuffer, ()>>;

/// Image loader
pub struct ImageLoader {
	/// Image request sender
	image_request_sender: priority_spmc::Sender<(ImageRequest, ImageResponder)>,

	/// Filesystem watcher
	_fs_watcher: notify::RecommendedWatcher,
}

impl ImageLoader {
	/// Creates a new image loader
	///
	/// # Errors
	/// Returns an error if unable to start the filesystem watcher
	pub fn new(
		base_path: PathBuf, loader_threads: Option<usize>, upscale_waifu2x: bool,
	) -> Result<Self, anyhow::Error> {
		// Start the watcher and start watching the path
		let (fs_tx, fs_rx) = mpsc::channel();
		let mut fs_watcher =
			notify::watcher(fs_tx, Duration::from_secs(2)).context("Unable to create directory watcher")?;
		fs_watcher
			.watch(&base_path, notify::RecursiveMode::Recursive)
			.context("Unable to start watching directory")?;

		// Start the distributer thread
		let (path_sender, path_receiver) = priority_spmc::channel();
		thread::Builder::new()
			.name("Path distributer".to_owned())
			.spawn(move || match self::path_distributer(&base_path, &path_sender, &fs_rx) {
				Ok(()) => log::debug!("Path distributer successfully quit"),
				Err(err) => log::warn!("Path distributer returned `Err`: {err:?}"),
			})
			.context("Unable to spawn distributer thread")?;

		// Start the image loader distribution thread
		let (image_request_sender, image_request_receiver) = priority_spmc::channel();
		let loader_threads = loader_threads.unwrap_or_else(default_loader_threads).max(1);
		for thread_idx in 0..loader_threads {
			let request_receiver = image_request_receiver.clone();
			let path_receiver = path_receiver.clone();
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


		Ok(Self {
			image_request_sender,
			_fs_watcher: fs_watcher,
		})
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

/// Path distributer thread function
///
/// Responsible for distributing paths to the image loader
fn path_distributer(
	base_path: &Path, path_sender: &priority_spmc::Sender<(PathBuf, PathResponder)>,
	fs_rx: &mpsc::Receiver<notify::DebouncedEvent>,
) -> Result<(), anyhow::Error> {
	// Load all paths
	let mut paths = vec![];
	self::scan_dir(&mut paths, base_path);

	// All response receivers
	let mut response_receivers: Vec<once_channel::Receiver<Result<(), LoadImageError>>> = vec![];

	// Start the reset-wait loop on our modifier
	loop {
		// Check if we have any filesystem events
		// Note: For rename and remove events, we simply ignore the
		//       file that no longer exists. The loader threads will
		//       mark the path for removal once they find it.
		while let Ok(event) = fs_rx.try_recv() {
			self::handle_fs_event(event, base_path, &mut paths);
		}

		// Check if we got any responses
		for receiver in mem::take(&mut response_receivers) {
			match receiver.try_recv() {
				// If everything went alright, don't do anything
				Ok(Ok(())) => (),
				// If we couldn't load the image, remove it
				// TODO: Maybe use some sort of ordered set to make this not perform as badly?
				Ok(Err(err)) => paths.retain(|path| path != &err.path),
				// If they're not done yet, push them back
				Err(once_channel::TryRecvError::NotReady(receiver)) => response_receivers.push(receiver),
				// If we couldn't get a response, ignore
				Err(_) => continue,
			}
		}

		// If we have no paths, wait for a filesystem event, or return, if unable to
		while paths.is_empty() {
			log::warn!("No paths found, waiting for new files from the filesystem watcher");
			match fs_rx.recv() {
				Ok(event) => self::handle_fs_event(event, base_path, &mut paths),
				Err(_) => anyhow::bail!("No paths are available and the filesystem watcher closed their channel"),
			}
		}

		// Then shuffle the paths we have
		log::trace!("Shuffling all files");
		paths.shuffle(&mut rand::thread_rng());

		// And request responses from them all
		for path in paths.iter().cloned() {
			// Create the channel for responding
			let (sender, receiver) = once_channel::channel();
			response_receivers.push(receiver);

			// Then send it and quit if we're done
			// Note: Priority for the path sender isn't mega relevant
			if path_sender.send((path, sender), 0).is_err() {
				return Ok(());
			}
		}
	}
}

/// Returns the default number of loader threads to use
fn default_loader_threads() -> usize {
	thread::available_parallelism().map_or(1, NonZeroUsize::get)
}

/// Handles a filesystem event
fn handle_fs_event(event: notify::DebouncedEvent, path: &Path, paths: &mut Vec<PathBuf>) {
	log::trace!("Receive filesystem event: {event:?}");

	#[allow(clippy::match_same_arms)] // They're logically in different parts
	match event {
		// Add the new path
		notify::DebouncedEvent::Create(path) | notify::DebouncedEvent::Rename(_, path) => {
			log::info!("Adding {path:?}");
			paths.push(path);
		},
		notify::DebouncedEvent::Remove(_) => (),

		// Clear all paths and rescan
		notify::DebouncedEvent::Rescan => {
			log::warn!("Re-scanning");
			paths.clear();
			self::scan_dir(paths, path);
		},

		// Note: Ignore any R/W events
		// TODO: Check if we should be doing this?
		notify::DebouncedEvent::NoticeWrite(_) |
		notify::DebouncedEvent::NoticeRemove(_) |
		notify::DebouncedEvent::Write(_) |
		notify::DebouncedEvent::Chmod(_) => (),

		// Log the error
		notify::DebouncedEvent::Error(err, path) => match path {
			Some(path) => log::warn!("Found error for path {path:?}: {:?}", anyhow::anyhow!(err)),
			None => log::warn!("Found error for unknown path: {:?}", anyhow::anyhow!(err)),
		},
	}
}

/// Image loader thread function
///
/// Responsible for receiving requests and loading them.
fn image_loader(
	request_receiver: &priority_spmc::Receiver<(ImageRequest, ImageResponder)>,
	path_receiver: &priority_spmc::Receiver<(PathBuf, PathResponder)>, upscale_waifu2x: bool,
) -> Result<(), anyhow::Error> {
	loop {
		// Get the path
		let (path, path_response_sender) = match path_receiver.recv() {
			Ok(path) => path,
			Err(_) => return Ok(()),
		};

		// Load the image
		let image = match load::load_image(&path) {
			Ok(image) => {
				log::debug!("Finished loading {path:?}");
				image
			},
			// If we didn't get it, send an error response and try again
			Err(err) => {
				log::info!("Unable to load {path:?}: {err:?}");
				if path_response_sender.send(Err(LoadImageError { path })).is_err() {
					return Ok(());
				}
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
				if path_response_sender.send(Ok(())).is_err() {
					return Ok(());
				}
			},
			// If we couldn't, send an error
			Err(err) => {
				log::info!("Unable to process {path:?}: {err:?}");
				let _ = sender.send(Err(()));
				if path_response_sender.send(Err(LoadImageError { path })).is_err() {
					return Ok(());
				}
			},
		};
	}
}

/// Scans a directory and insert all it's paths onto `paths`
fn scan_dir(paths: &mut Vec<PathBuf>, path: &Path) {
	let mut visitor = |path| {
		paths.push(path);
		ControlFlow::CONTINUE
	};
	self::visit_files_dir::<!, _>(path, &mut visitor).into_ok();
}

/// Visits all files in `path`, recursively.
///
/// # Errors
/// Ignores all errors reading directories, simply logging them.
///
/// # Return
/// Returns the number of files successfully loaded
fn visit_files_dir<E, F>(path: &Path, f: &mut F) -> Result<usize, E>
where
	F: FnMut(PathBuf) -> ControlFlow<E>,
{
	let mut files_loaded = 0;
	let dir = match std::fs::read_dir(path) {
		Ok(dir) => dir,
		Err(err) => {
			log::warn!("Unable to read directory `{path:?}`: {:?}", anyhow::anyhow!(err));
			return Ok(0);
		},
	};
	for entry in dir {
		// Read the entry and file type
		let entry = match entry {
			Ok(entry) => entry,
			Err(err) => {
				log::warn!("Unable to read file entry in `{path:?}`: {:?}", anyhow::anyhow!(err));
				continue;
			},
		};
		let entry_path = entry.path();
		let file_type = match entry.file_type() {
			Ok(file_type) => file_type,
			Err(err) => {
				log::warn!(
					"Unable to read file type for `{entry_path:?}`: {:?}",
					anyhow::anyhow!(err)
				);
				continue;
			},
		};

		match file_type.is_dir() {
			// Recurse on directories
			true => {
				files_loaded += self::visit_files_dir(&entry.path(), f)?;
			},

			// Visit files
			false => match f(entry_path) {
				ControlFlow::Continue(()) => files_loaded += 1,
				ControlFlow::Break(err) => return Err(err),
			},
		}
	}

	Ok(files_loaded)
}
