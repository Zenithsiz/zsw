//! Image loader

// Imports
use {
	anyhow::Context,
	image::{io::Reader as ImageReader, DynamicImage},
	std::{fs, io::BufReader, path::Path},
};

/// Loads an image from a path
pub fn load_image(path: &Path) -> Result<DynamicImage, anyhow::Error> {
	// Canonicalize the path before loading
	let path = path.canonicalize().context("Unable to canonicalize path")?;

	// Open the image
	let image_file = fs::File::open(&path).context("Unable to open image file")?;
	let mut image_file = BufReader::new(image_file);

	// Then guess it's format
	let format = self::image_format(&mut image_file)?;

	// Else, just read the image
	ImageReader::with_format(&mut image_file, format)
		.decode()
		.context("Unable to decode image")
}

/// Returns the image format of `image_file`
fn image_format(image_file: &mut BufReader<fs::File>) -> Result<image::ImageFormat, anyhow::Error> {
	ImageReader::new(image_file)
		.with_guessed_format()
		.context("Unable to guess image format")?
		.format()
		.context("Image format not supported")
}
