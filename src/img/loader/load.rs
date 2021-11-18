//! Image loader

// Imports
use anyhow::Context;
use image::DynamicImage;
use std::path::Path;

/// Loads an image from a path
pub fn load_image(path: &Path) -> Result<DynamicImage, anyhow::Error> {
	// Try to open the image by guessing it's format
	let image_reader = image::io::Reader::open(&path)
		.context("Unable to open image")?
		.with_guessed_format()
		.context("Unable to parse image")?;
	image_reader.decode().context("Unable to decode image")
}
