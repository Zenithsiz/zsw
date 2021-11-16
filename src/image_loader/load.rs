//! Image loader

use anyhow::Context;
use image::{imageops::FilterType, GenericImageView};
use num_rational::Ratio;

// Imports
use crate::ImageBuffer;
use std::{cmp::Ordering, path::Path};

/// Loads an image from a path given the window it's going to be displayed in
pub fn load_image(path: &Path, [window_width, window_height]: [u32; 2]) -> Result<ImageBuffer, anyhow::Error> {
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
	log::trace!("{scroll_dir:?} - {image_width:?}x{image_height:?} -> {window_width:?}x{window_height:?}");
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
