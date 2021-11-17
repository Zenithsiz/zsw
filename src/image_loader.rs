//! Image loader
//!
//! See the [`ImageLoader`] type for more details on how image loading
//! works.

// Modules
mod load;
mod paths;
mod request;

// Exports
pub use self::request::{ImageRequest, ResponseError};

// Imports
use crate::sync::{once_channel, rrc};
use anyhow::Context;
use cgmath::Vector2;
use image::Rgba;
use notify::Watcher;
use rand::prelude::SliceRandom;
use std::{
	num::NonZeroUsize,
	ops::ControlFlow,
	path::{Path, PathBuf},
	sync::mpsc,
	thread::{self, JoinHandle},
	time::Duration,
};

/// Image buffer
pub type ImageBuffer = image::ImageBuffer<Rgba<u8>, Vec<u8>>;

/// Image loader
///
/// Responsible for loading all images from a directory and
/// supplying them.
pub struct ImageLoader {
	/// Paths receiver
	paths_rx: paths::Receiver,

	/// Request-receiver channel to request images on the distributer
	requester: rrc::Requester<ImageRequest, once_channel::Receiver<Result<ImageBuffer, ResponseError>>>,

	/// All loader threads
	loader_threads: Vec<JoinHandle<()>>,

	/// Distributer thread
	distributer_thread: JoinHandle<()>,
}

impl ImageLoader {
	/// Creates a new image loader and starts loading images in background threads.
	///
	/// # Errors
	/// Returns error if unable to create a directory watcher.
	// TODO: Somehow allow different window sizes per images by asking and giving out tokens or something?
	// TODO: Add a max-threads parameter
	pub fn new(path: PathBuf, loader_threads: Option<usize>, upscale_waifu2x: bool) -> Result<Self, anyhow::Error> {
		// Create the modify-receive channel with all of the initial images
		// Note: we also shuffle them here at the beginning
		let (paths_modifier, paths_rx) = {
			let mut paths = vec![];
			scan_dir(&mut paths, &path);
			paths.shuffle(&mut rand::thread_rng());
			paths::channel(paths)
		};

		// Then start all loading threads
		let (requester, responder) = rrc::channel();
		let loader_threads =
			loader_threads.unwrap_or_else(|| thread::available_parallelism().map_or(1, NonZeroUsize::get));
		let loader_threads = (0..loader_threads)
			.map(|thread_idx| {
				let responder = responder.clone();
				thread::spawn(move || match self::image_loader(responder, upscale_waifu2x) {
					Ok(()) => log::debug!("Image loader #{thread_idx} successfully quit"),
					Err(err) => log::warn!("Image loader #{thread_idx} returned `Err`: {err:?}"),
				})
			})
			.collect();

		// And start the image distributer thread
		let distributer_thread = thread::spawn(move || match image_distributer(&path, &paths_modifier) {
			Ok(()) => log::debug!("Image distributer successfully quit"),
			Err(err) => log::error!("Image distributer returned `Err`: {err:?}"),
		});

		Ok(Self {
			paths_rx,
			requester,
			loader_threads,
			distributer_thread,
		})
	}

	/// Queues a new image to be processed at a given resolution
	///
	/// # Errors
	/// Returns an error if unable to queue the image
	pub fn queue_image(
		&self, size: Vector2<u32>,
	) -> Result<once_channel::Receiver<Result<ImageBuffer, ResponseError>>, anyhow::Error> {
		// Get the path from the distributer
		let (idx, path) = self.paths_rx.recv().context("Distributer thread quit")?;

		// Then send a request
		Ok(self.requester.request_wait(ImageRequest { size, path, idx }))
	}

	/// Returns a failed request to be removed
	pub fn on_failed_request(&self, err: &ResponseError) {
		self.paths_rx.remove(err.idx);
	}

	/// Joins all loader threads and the distributer thread
	///
	/// # Errors
	/// Returns an error if unable to join a loader thread or the distributer thread
	pub fn join_all(self) -> Result<(), anyhow::Error> {
		// Drop our image receiver
		//mem::drop(self.image_rx);

		// Then join all the loader threads
		for (thread_idx, loader_thread) in self.loader_threads.into_iter().enumerate() {
			log::debug!("Joining loader thread #{thread_idx}");
			loader_thread
				.join()
				.map_err(|_| anyhow::anyhow!("Unable to join loader thread #{thread_idx}"))?;
		}

		// And finally join the distributer thread
		log::debug!("Joining distributer thread");
		self.distributer_thread
			.join()
			.map_err(|_| anyhow::anyhow!("Unable to join distributer thread"))?;

		Ok(())
	}
}

/// Image distributer for all loader threads
fn image_distributer(path: &Path, modifier: &paths::Modifier) -> Result<(), anyhow::Error> {
	// Start the watcher and start watching the path
	let (fs_tx, fs_rx) = mpsc::channel();
	let mut watcher = notify::watcher(fs_tx, Duration::from_secs(2)).context("Unable to create directory watcher")?;
	watcher
		.watch(&path, notify::RecursiveMode::Recursive)
		.context("Unable to start watching directory")?;

	// Start the reset-wait loop on our modifier
	modifier.reset_wait_loop(|paths| {
		// Check if we have any filesystem events
		// Note: For rename and remove events, we simply ignore the
		//       file that no longer exists. The loader threads will
		//       mark the path for removal once they find it.
		while let Ok(event) = fs_rx.try_recv() {
			self::handle_fs_event(event, path, paths);
		}

		// If we have no paths, wait for a filesystem event, or return, if unable to
		while paths.is_empty() {
			log::warn!("No paths found, waiting for new files from the filesystem watcher");
			match fs_rx.recv() {
				Ok(event) => self::handle_fs_event(event, path, paths),
				Err(_) => anyhow::bail!("No paths are available and the filesystem watcher closed their channel"),
			}
		}

		// Then shuffle the paths we have and send them to each loader thread
		log::trace!("Shuffling all files");
		paths.shuffle(&mut rand::thread_rng());

		Ok(())
	})
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

/// Image loader to run in a background thread
#[allow(clippy::needless_pass_by_value)] // It's better for this function to own the sender
fn image_loader(
	responder: rrc::Responder<ImageRequest, once_channel::Receiver<Result<ImageBuffer, ResponseError>>>,
	upscale_waifu2x: bool,
) -> Result<(), anyhow::Error> {
	loop {
		// Wait for a request
		let (sender, request) = responder.wait_request(|request| {
			let (sender, receiver) = once_channel::channel();
			((sender, request), receiver)
		});

		// Then try to load it
		let image = match load::load_image(&request.path, request.size, upscale_waifu2x) {
			Ok(value) => value,
			// Return `Err` if we couldn't
			Err(err) => {
				log::info!("Unable to load {:?}: {err:?}", request.path);
				if sender.send(Err(ResponseError { idx: request.idx })).is_err() {
					break Ok(());
				}
				continue;
			},
		};

		// And try to send it, or join and return `Ok()` if we're no longer sending images
		if sender.send(Ok(image)).is_err() {
			// TODO: Join?
			break Ok(());
		}
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
