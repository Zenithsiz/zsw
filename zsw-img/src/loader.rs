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
pub struct ImageLoader<P: RawImageProvider> {
	/// Image sender
	image_tx: crossbeam::channel::Sender<Image<P>>,

	/// To remove image receiver
	to_remove_image_rx: crossbeam::channel::Receiver<Image<P>>,
}

impl<P: RawImageProvider> ImageLoader<P> {
	/// Runs this image loader
	///
	/// Multiple image loaders may run at the same time
	pub fn run(self, provider: &P) {
		'run: loop {
			// If we have any images to remove, remove them
			// Note: We don't care if the sender quit for this channel
			while let Ok(image) = self.to_remove_image_rx.try_recv() {
				provider.remove_image(image.raw_image_token);
			}

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
						raw_image_token: raw_image.into_token(),
					};

					if self.image_tx.send(image).is_err() {
						tracing::debug!("Quitting image loader: Receiver quit");
						break 'run;
					}
				},

				// If we couldn't load, log, remove the path and retry
				Err(err) => {
					tracing::info!(name = ?raw_image.name(), ?err, "Unable to load image");
					provider.remove_image(raw_image.into_token());
				},
			}
		}
	}
}

/// Image downscale service
#[derive(Clone, Debug)]
pub struct ImageDownscaler<P: RawImageProvider> {
	/// Downscaled image sender
	downscaled_image_tx: crossbeam::channel::Sender<Image<P>>,

	/// To downscale image receiver
	// TODO: Proper type for this instead of tuple
	to_downscale_image_rx: crossbeam::channel::Receiver<(Image<P>, u32)>,
}

impl<P: RawImageProvider> ImageDownscaler<P> {
	/// Runs this image downscaler
	///
	/// Multiple image downscalers may run at the same time
	pub fn run(self) {
		'run: loop {
			// Get the next image, or quit if no more
			let Ok((mut image, max_size)) = self.to_downscale_image_rx.recv() else {
				break;
			};

			// Downscale it and send it
			self::downscale_image(&mut image, max_size);
			if self.downscaled_image_tx.send(image).is_err() {
				tracing::debug!("Quitting image downscaler: Receiver quit");
				break 'run;
			}
		}
	}
}

/// Image receiver
#[derive(Debug)]
pub struct ImageReceiver<P: RawImageProvider> {
	/// Image receiver
	image_rx: crossbeam::channel::Receiver<Image<P>>,

	/// To remove image sender
	to_remove_image_tx: crossbeam::channel::Sender<Image<P>>,

	/// To downscale image sender
	to_downscale_image_tx: crossbeam::channel::Sender<(Image<P>, u32)>,
}

impl<P: RawImageProvider> ImageReceiver<P> {
	/// Attempts to receive the image
	// TODO: Return an error when receiver quit
	#[must_use]
	pub fn try_recv(&self) -> Option<Image<P>> {
		self.image_rx.try_recv().ok()
	}

	/// Queues an image to be removed
	pub fn queue_remove(&self, image: Image<P>) -> Result<(), Image<P>> {
		self.to_remove_image_tx.send(image).map_err(|err| err.0)
	}

	/// Queues an image to be downscaled
	pub fn queue_downscale(&self, image: Image<P>, max_size: u32) -> Result<(), Image<P>> {
		self.to_downscale_image_tx
			.send((image, max_size))
			.map_err(|err| err.0 .0)
	}
}

/// Raw image provider
pub trait RawImageProvider {
	/// Raw image type
	type RawImage: RawImage;

	/// Provides the next image, if any
	fn next_image(&self) -> Option<Self::RawImage>;

	/// Removes an image
	fn remove_image(&self, token: <Self::RawImage as RawImage>::Token);
}

/// Raw image
pub trait RawImage {
	/// Image Reader
	type Reader<'a>: io::BufRead + io::Seek
	where
		Self: 'a;

	/// Image token type.
	///
	/// Used to identify the image. Will be stored along-side
	/// the loaded image for it's lifecycle, so should not contain
	/// anything "heavy" (e.g. an open file)
	type Token: std::fmt::Debug;

	/// Returns this image's reader
	fn reader(&mut self) -> Self::Reader<'_>;

	/// Returns this image's name
	fn name(&self) -> &str;

	/// Consumes this raw image into a token representing it
	fn into_token(self) -> Self::Token;
}

/// Downscales an image
// TODO: Allow max size for both width and height?
#[allow(clippy::cast_sign_loss)] // We're sure it's positive
pub fn downscale_image<P: RawImageProvider>(image: &mut Image<P>, max_size: u32) {
	let width = image.image.width();
	let height = image.image.height();
	let downscale_factor = f32::min(max_size as f32 / width as f32, max_size as f32 / height as f32);
	let downscaled_width = (width as f32 * downscale_factor) as u32;
	let downscaled_height = (height as f32 * downscale_factor) as u32;

	assert_le!(downscaled_width, max_size, "Calculated width is too large");
	assert_le!(downscaled_height, max_size, "Calculated height is too large");

	// TODO: What filter to use here?
	image.image = image.image.resize(
		downscaled_width,
		downscaled_height,
		image::imageops::FilterType::Lanczos3,
	);
	tracing::debug!(
		"Downscaled {}: {width}x{height} to {}x{} ({:.2}%)",
		image.name,
		image.image.width(),
		image.image.height(),
		downscale_factor * 100.0
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
pub fn create<P: RawImageProvider>() -> (ImageLoader<P>, ImageDownscaler<P>, ImageReceiver<P>) {
	// Create the image channel
	// Note: We have the lowest possible bound on the receiver due to images being quite big
	// Note: Since the downscale / remover is on-demand, we use an unbounded buffer to avoid waiting
	//       queueing requests on receiving the images.
	// TODO: Make this customizable?
	let (image_tx, image_rx) = crossbeam::channel::bounded(0);
	let (to_downscale_image_tx, to_downscale_image_rx) = crossbeam::channel::unbounded();
	let (to_remove_image_tx, to_remove_image_rx) = crossbeam::channel::unbounded();

	(
		ImageLoader {
			image_tx: image_tx.clone(),
			to_remove_image_rx,
		},
		ImageDownscaler {
			downscaled_image_tx: image_tx,
			to_downscale_image_rx,
		},
		ImageReceiver {
			image_rx,
			to_remove_image_tx,
			to_downscale_image_tx,
		},
	)
}
