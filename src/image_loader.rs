//! Image loader

// Imports
use anyhow::Context;
use image::{imageops::FilterType, GenericImageView, Rgba};
use notify::Watcher;
use num_rational::Ratio;
use rand::prelude::SliceRandom;
use std::{
	cmp::Ordering,
	num::NonZeroUsize,
	ops::ControlFlow,
	path::{Path, PathBuf},
	sync::mpsc::{self, RecvError, SendError},
	thread,
	time::Duration,
};

/// Image buffer
pub type ImageBuffer = image::ImageBuffer<Rgba<u8>, Vec<u8>>;

/// Image loader
///
/// Responsible for loading images from the given directory and
/// supplying it them once ready.
///
/// ## Architecture
/// The current architecture uses N + 1 background threads, where
/// N is the `available_parallelism`. The N threads receive an image
/// path, load them, and send the image across a channel back to this image loader.
/// The 1 thread is responsible for assigning the image paths to each thread.
pub struct ImageLoader {
	/// Receiver end for the image loading.
	image_rx: mpsc::Receiver<ImageBuffer>,

	/// Watcher
	_watcher: notify::RecommendedWatcher,
}

impl ImageLoader {
	/// Creates a new image loader and starts loading images in background threads.
	///
	/// # Errors
	/// Returns error if unable to create a directory watcher.
	// TODO: Add a max-threads parameter
	pub fn new(path: PathBuf, image_backlog: usize, window_size: [u32; 2]) -> Result<Self, anyhow::Error> {
		// Create the event channel, for transmitting filesystem
		// events to every thread
		let (event_tx, event_rx) = mpsc::channel();

		// Then start the watcher and start watching the path
		let mut watcher =
			notify::watcher(event_tx.clone(), Duration::from_secs(2)).context("Unable to create directory watcher")?;
		watcher
			.watch(&path, notify::RecursiveMode::Recursive)
			.context("Unable to start watching directory")?;

		// And send existing files over the sender
		thread::spawn(move || {
			let mut visitor = |path| match event_tx.send(notify::DebouncedEvent::Create(path)) {
				Ok(()) => ControlFlow::CONTINUE,
				Err(_) => ControlFlow::Break(()),
			};
			match self::visit_files_dir(&path, &mut visitor) {
				Ok(files_loaded) => log::info!("Found {files_loaded} files"),
				Err(()) => (),
			}
		});

		// Start all loading threads
		let (image_tx, image_rx) = mpsc::sync_channel(image_backlog);
		let available_parallelism = thread::available_parallelism().map_or(1, NonZeroUsize::get);
		let loader_event_txs = (0..available_parallelism)
			.map(|thread_idx| {
				let image_tx = image_tx.clone();
				let (loader_event_tx, loader_event_rx) = mpsc::channel();
				thread::spawn(
					move || match self::image_loader(loader_event_rx, window_size, image_tx, thread_idx) {
						Ok(never) => never,
						Err(err) => log::error!("Loader thread #{thread_idx} returned error: {err:?}"),
					},
				);
				loader_event_tx
			})
			.collect::<Vec<_>>();

		// Star the image distributer thread
		thread::spawn(move || image_distributer(loader_event_txs, event_rx));

		Ok(Self {
			image_rx,
			_watcher: watcher,
		})
	}

	/// Returns the next image, waiting if not yet available
	///
	/// # Errors
	/// Returns an error if all loader threads exited
	pub fn next_image(&mut self) -> Result<ImageBuffer, anyhow::Error> {
		self.image_rx.recv().context("All loader threads exited")
	}

	/// Returns the next image, returning `None` if not yet loaded
	///
	/// # Errors
	/// Returns an error if all loader threads exited
	pub fn try_next_image(&mut self) -> Result<Option<ImageBuffer>, anyhow::Error> {
		match self.image_rx.try_recv() {
			// if we got it, return it
			Ok(image) => Ok(Some(image)),

			// If it wasn't ready, return `None`
			Err(mpsc::TryRecvError::Empty) => Ok(None),

			// If unable to, wait and increase the timeout
			Err(mpsc::TryRecvError::Disconnected) => anyhow::bail!("All loader threads exited"),
		}
	}
}

/// Image distributer for all background threads
#[allow(clippy::needless_pass_by_value)] // There is no one else to own them, so we can own them.
fn image_distributer(
	loader_event_txs: Vec<mpsc::Sender<notify::DebouncedEvent>>, event_rx: mpsc::Receiver<notify::DebouncedEvent>,
) {
	// All paths we have
	//let mut paths = vec![];

	'main_loop: loop {
		for loader_event_tx in &loader_event_txs {
			match event_rx.recv() {
				Ok(value) => match loader_event_tx.send(value) {
					Ok(()) => (),
					Err(_) => break 'main_loop,
				},
				Err(_) => break 'main_loop,
			}
		}
	}

	log::info!("Image distributer thread exiting");
}

/// Image loader to run in a background thread
#[allow(clippy::needless_pass_by_value)] // It's better for this function to own the sender
fn image_loader(
	event_rx: mpsc::Receiver<notify::DebouncedEvent>, window_size: [u32; 2], image_tx: mpsc::SyncSender<ImageBuffer>,
	thread_idx: usize,
) -> Result<!, ImageLoaderError> {
	let mut paths = vec![];

	loop {
		// Receives the next event, waiting if we're empty
		let next_event = |is_empty| match is_empty {
			true => {
				log::warn!("#{thread_idx}: No images found, waiting for new files");
				Ok(Some(event_rx.recv()?))
			},
			false => match event_rx.try_recv() {
				Ok(path) => Ok(Some(path)),
				Err(mpsc::TryRecvError::Disconnected) => Err(mpsc::RecvError),
				Err(mpsc::TryRecvError::Empty) => Ok(None),
			},
		};

		// Check for new paths, or, if we're out, wait
		while let Some(event) = next_event(paths.is_empty()).map_err(ImageLoaderError::ReceiveEvent)? {
			// Note: No need to match `Remove`, the `drain_filter` below will remove it.
			// Note: On `Rename`, the original path will be removed by the `drain_filter` below
			match event {
				notify::DebouncedEvent::Create(path) | notify::DebouncedEvent::Rename(_, path) => {
					log::trace!("#{thread_idx}: Adding {path:?}");
					paths.push(path);
				},
				notify::DebouncedEvent::Error(err, path) => {
					log::warn!("#{thread_idx}: Received error from directory watcher for {path:?}: {err:?}");
				},
				_ => (),
			}
		}

		// Shuffles all paths
		log::trace!("#{thread_idx}: Shuffling all files");
		paths.shuffle(&mut rand::thread_rng());

		// Then load them all and send them
		let mut send_err = None;
		paths.drain_filter(|path| {
			// If we have a sending error, just return
			if send_err.is_some() {
				return false;
			}

			// ELse try to load it
			let image = match self::load_image(path, window_size) {
				Ok(value) => value,
				Err(err) => {
					log::info!("#{thread_idx}: Unable to load {path:?}: {err:?}");
					return true;
				},
			};

			// Then try to send it
			if let Err(err) = image_tx.send(image) {
				send_err = Some(err);
			}

			false
		});

		// If we got a send error, return Err
		if let Some(err) = send_err {
			return Err(ImageLoaderError::SendImage(err));
		}
	}
}

#[derive(Debug)]
enum ImageLoaderError {
	/// Unable to send image
	SendImage(SendError<ImageBuffer>),

	/// Unable to receive fs event
	ReceiveEvent(RecvError),
}

/// Loads an image from a path
fn load_image(path: &Path, [window_width, window_height]: [u32; 2]) -> Result<ImageBuffer, anyhow::Error> {
	// Try to open the image by guessing it's format
	let image_reader = image::io::Reader::open(&path)
		.context("Unable to open image")?
		.with_guessed_format()
		.context("Unable to parse image")?;
	let image = image_reader.decode().context("Unable to decode image")?;

	// Get it's width and aspect ratio
	let (image_width, image_height) = (image.width(), image.height());
	let image_aspect_ratio = Ratio::new(image_width, image_height);
	let window_aspect_ratio = Ratio::new(window_width, window_height);

	log::trace!("Loaded {path:?} ({image_width}x{image_height})");

	// Then check what direction we'll be scrolling the image
	let scroll_dir = match (image_width.cmp(&image_height), window_width.cmp(&window_height)) {
		// If they're both square, no scrolling occurs
		(Ordering::Equal, Ordering::Equal) => ScrollDir::None,

		// Else if the image is tall and the window is wide, it must scroll vertically
		(Ordering::Less | Ordering::Equal, Ordering::Greater | Ordering::Equal) => ScrollDir::Vertically,

		// Else if the image is wide and the window is tall, it must scroll horizontally
		(Ordering::Greater | Ordering::Equal, Ordering::Less | Ordering::Equal) => ScrollDir::Horizontally,

		// Else we need to check the aspect ratio
		(Ordering::Less, Ordering::Less) | (Ordering::Greater, Ordering::Greater) => {
			match image_aspect_ratio.cmp(&window_aspect_ratio) {
				// If the image is wider than the screen, we'll scroll horizontally
				Ordering::Greater => ScrollDir::Horizontally,

				// Else if the image is taller than the screen, we'll scroll vertically
				Ordering::Less => ScrollDir::Vertically,

				// Else if they're equal, no scrolling occurs
				Ordering::Equal => ScrollDir::None,
			}
		},
	};
	log::trace!("Scrolling image with directory: {scroll_dir:?}");

	// Then get the size we'll be resizing to, if any
	let resize_size = match scroll_dir {
		// If we're scrolling vertically, resize if the image width is larger than the window width
		ScrollDir::Vertically if image_width > window_width => {
			Some((window_width, (window_width * image_height) / image_width))
		},

		// If we're scrolling horizontally, resize if the image height is larger than the window height
		ScrollDir::Horizontally if image_height > window_height => {
			Some(((window_height * image_width) / image_height, window_height))
		},

		// If we're not doing any scrolling and the window is smaller, resize the image to screen size
		// Note: Since we're not scrolling, we know aspect ratio is the same and so
		//       we only need to check the width.
		ScrollDir::None if image_width > window_width => Some((window_width, window_height)),

		// Else don't do any scrolling
		_ => None,
	};

	// And resize if necessary
	let image = match resize_size {
		Some((resize_width, resize_height)) => {
			let reduction = 100.0 * (f64::from(resize_width) * f64::from(resize_height)) /
				(f64::from(image_width) * f64::from(image_height));

			log::trace!(
				"Resizing from {image_width}x{image_height} to {resize_width}x{resize_height} ({reduction:.2}%)",
			);
			image.resize_exact(resize_width, resize_height, FilterType::Lanczos3)
		},
		None => {
			log::trace!("Not resizing");
			image
		},
	};

	let image = image.flipv().to_rgba8();
	Ok(image)
}

/// Image scrolling direction
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum ScrollDir {
	Vertically,
	Horizontally,
	None,
}


/// Visits all files in `path`, recursively.
///
/// # Errors
/// Ignores all errors reading directories, simply logging them.
///
/// # Return
/// Returns the number of files successfully loaded
fn visit_files_dir<E>(path: &Path, f: &mut impl FnMut(PathBuf) -> ControlFlow<E>) -> Result<usize, E> {
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
