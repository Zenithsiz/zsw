//! Image loader
//!
//! See the [`ImageLoader`] type for more details on how image loading
//! works.

// Imports
use {
	super::Image,
	anyhow::Context,
	image::{io::Reader as ImageReader, DynamicImage},
	more_asserts::assert_le,
	std::io,
};

/// Image loader service
#[derive(Clone, Debug)]
pub struct ImageLoader {
	/// Image sender
	image_tx: crossbeam::channel::Sender<Image>,
}

impl ImageLoader {
	/// Runs this image loader
	///
	/// Multiple image loaders may run at the same time
	pub fn run<P: RawImageProvider>(self, provider: &P) {
		'run: loop {
			// Get the next raw image, or quit if no more
			let Some(mut raw_image) = provider.next_image() else {
				break;
			};

			// Try to load the image
			let res = self::load_image(&mut raw_image.reader());
			match res {
				// If we got it, send it
				Ok(image) => {
					let image_name = raw_image.name().to_owned();
					tracing::trace!(
						"Loaded image {image_name}: ({} {}x{})",
						zsw_util::image_format(&image),
						image.width(),
						image.height()
					);
					let image = Image {
						name: image_name,
						image,
					};

					if self.image_tx.send(image).is_err() {
						tracing::debug!("Quitting image loader: Receiver quit");
						break 'run;
					}
				},

				// If we couldn't load, log, remove the path and retry
				Err(err) => {
					tracing::info!(name = ?raw_image.name(), ?err, "Unable to load image");
					provider.remove_image(&raw_image);
				},
			}
		}
	}
}

/// Image resizer service
#[derive(Clone, Debug)]
pub struct ImageResizer {
	/// Resized image sender
	resized_image_tx: crossbeam::channel::Sender<Image>,

	/// To resize image receiver
	// TODO: Proper type for this instead of tuple
	to_resize_image_rx: crossbeam::channel::Receiver<(Image, u32)>,
}

impl ImageResizer {
	/// Runs this image resizer
	///
	/// Multiple image resizers may run at the same time
	pub fn run(self) {
		'run: loop {
			// Get the next image, or quit if no more
			let Ok((mut image, max_size)) = self.to_resize_image_rx.recv() else {
				break;
			};

			// Resize it and send it
			self::resize_image(&mut image, max_size);
			if self.resized_image_tx.send(image).is_err() {
				tracing::debug!("Quitting image resizer: Receiver quit");
				break 'run;
			}
		}
	}
}

/// Image receiver
#[derive(Debug)]
pub struct ImageReceiver {
	/// Image receiver
	image_rx: crossbeam::channel::Receiver<Image>,

	/// To resize image sender
	to_resize_image_tx: crossbeam::channel::Sender<(Image, u32)>,
}

impl ImageReceiver {
	/// Attempts to receive the image
	// TODO: Return an error when receiver quit
	#[must_use]
	pub fn try_recv(&self) -> Option<Image> {
		self.image_rx.try_recv().ok()
	}

	/// Queues an image to be resized
	pub fn queue_resize(&self, image: Image, max_size: u32) -> Result<(), Image> {
		self.to_resize_image_tx.send((image, max_size)).map_err(|err| err.0 .0)
	}
}

/// Raw image provider
pub trait RawImageProvider {
	/// Raw image type
	type RawImage: RawImage;

	/// Provides the next image, if any
	fn next_image(&self) -> Option<Self::RawImage>;

	/// Removes an image
	fn remove_image(&self, raw_image: &Self::RawImage);
}

/// Raw image
pub trait RawImage {
	/// Image Reader
	type Reader<'a>: io::BufRead + io::Seek
	where
		Self: 'a;

	/// Returns this image's reader
	fn reader(&mut self) -> Self::Reader<'_>;

	/// Returns this image's name
	fn name(&self) -> &str;
}

/// Resizes an image
// TODO: Allow max size for both width and height?
#[allow(clippy::cast_sign_loss)] // We're sure it's positive
pub fn resize_image(image: &mut Image, max_size: u32) {
	let width_f32 = image.image.width() as f32;
	let height_f32 = image.image.height() as f32;
	let resize_factor = f32::min(max_size as f32 / width_f32, max_size as f32 / height_f32);
	let resize_width = (width_f32 * resize_factor) as u32;
	let resize_height = (height_f32 * resize_factor) as u32;

	assert_le!(resize_width, max_size, "Calculated width is too large");
	assert_le!(resize_height, max_size, "Calculated height is too large");

	// TODO: What filter to use here?
	image.image = image
		.image
		.resize(resize_width, resize_height, image::imageops::FilterType::Lanczos3);
	tracing::warn!(
		"Resized {}: {}x{} to {resize_width}x{resize_height} ({:.2}%)",
		image.name,
		image.image.width(),
		image.image.height(),
		resize_factor * 100.0
	);
}

/// Loads an image from a path
fn load_image<R: io::BufRead + io::Seek>(raw_image: &mut R) -> Result<DynamicImage, anyhow::Error> {
	// Then guess it's format
	let format = self::image_format(raw_image)?;

	// Else, just read the image
	ImageReader::with_format(raw_image, format)
		.decode()
		.context("Unable to decode image")
}

/// Returns the image format of `raw_image`
fn image_format<R: io::BufRead + io::Seek>(raw_image: &mut R) -> Result<image::ImageFormat, anyhow::Error> {
	ImageReader::new(raw_image)
		.with_guessed_format()
		.context("Unable to guess image format")?
		.format()
		.context("Image format not supported")
}


/// Creates the image loader service
#[must_use]
pub fn create() -> (ImageLoader, ImageResizer, ImageReceiver) {
	// Create the image channel
	// Note: We have the lowest possible bound on the receiver due to images being quite big
	// Note: Since the resizer is on-demand, we use an unbounded buffer to avoid waiting
	//       queueing requests on receiving the images.
	// TODO: Make this customizable?
	let (image_tx, image_rx) = crossbeam::channel::bounded(0);
	let (to_resize_image_tx, to_resize_image_rx) = crossbeam::channel::unbounded();

	(
		ImageLoader {
			image_tx: image_tx.clone(),
		},
		ImageResizer {
			resized_image_tx: image_tx,
			to_resize_image_rx,
		},
		ImageReceiver {
			image_rx,
			to_resize_image_tx,
		},
	)
}
