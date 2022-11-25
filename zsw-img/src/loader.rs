//! Image loader
//!
//! See the [`ImageLoader`] type for more details on how image loading
//! works.

// Imports
use {
	super::Image,
	anyhow::Context,
	image::{io::Reader as ImageReader, DynamicImage},
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

/// Image receiver
#[derive(Debug)]
pub struct ImageReceiver {
	/// Image receiver
	image_rx: crossbeam::channel::Receiver<Image>,
}


impl ImageReceiver {
	/// Attempts to receive the image
	#[must_use]
	pub fn try_recv(&self) -> Option<Image> {
		self.image_rx.try_recv().ok()
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

/// Creates the image loader service
#[must_use]
pub fn create() -> (ImageLoader, ImageReceiver) {
	// Create the image channel
	// Note: We have the lowest possible bound due to images being quite big
	// TODO: Make this customizable?
	let (image_tx, image_rx) = crossbeam::channel::bounded(0);

	(ImageLoader { image_tx }, ImageReceiver { image_rx })
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
