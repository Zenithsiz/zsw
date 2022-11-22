//! Image loader

// Imports
use {
	anyhow::Context,
	image::{io::Reader as ImageReader, DynamicImage},
	std::io,
};

/// Loads an image from a path
pub fn load_image<R: io::BufRead + io::Seek>(raw_image: &mut R) -> Result<DynamicImage, anyhow::Error> {
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
