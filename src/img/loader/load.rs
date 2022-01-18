//! Image loader

// Modules

// Imports
use crate::img::ImageRequest;
use anyhow::Context;
use cgmath::Vector2;
use image::{io::Reader as ImageReader, DynamicImage};
use std::{
	fs,
	io::{BufReader, Seek},
	path::Path,
};

/// Loads an image from a path
pub fn load_image(path: &Path, _request: ImageRequest) -> Result<DynamicImage, anyhow::Error> {
	// Canonicalize the path before loading
	let path = path.canonicalize().context("Unable to canonicalize path")?;

	// Open the image
	let image_file = fs::File::open(&path).context("Unable to open image file")?;
	let mut image_file = BufReader::new(image_file);

	// Then guess it's format
	let format = self::image_format(&mut image_file)?;

	// Get it's size and others
	let _image_size = self::image_size(&mut image_file, format)?;

	// Else, just read the image
	ImageReader::with_format(&mut image_file, format)
		.decode()
		.context("Unable to decode image")
}

/// Returns the image format of `image_file`
// TODO: Something less janky than this
fn image_format(image_file: &mut BufReader<fs::File>) -> Result<image::ImageFormat, anyhow::Error> {
	ImageReader::new(image_file)
		.with_guessed_format()
		.context("Unable to guess image format")?
		.format()
		.context("Image format not supported")
}

/// Returns the image size of `image_file` given it's format
fn image_size(image_file: &mut BufReader<fs::File>, format: image::ImageFormat) -> Result<Vector2<u32>, anyhow::Error> {
	let (width, height) = ImageReader::with_format(&mut *image_file, format)
		.into_dimensions()
		.context("Unable to get image dimensions")?;
	image_file.rewind().context("Unable to rewind file")?;
	Ok(Vector2::new(width, height))
}
