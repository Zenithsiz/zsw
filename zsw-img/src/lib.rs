//! Image handling

// Features
#![feature(never_type)]

// Modules
pub mod loader;

// Exports
pub use loader::ImageLoader;

// Imports
use {
	cgmath::Vector2,
	image::{DynamicImage, GenericImageView},
	std::path::PathBuf,
};

/// Loaded image
#[derive(Debug)]
pub struct Image {
	/// Path of the image
	pub path: PathBuf,

	/// Image
	pub image: DynamicImage,
}

impl Image {
	/// Returns the image's size
	#[must_use]
	pub fn size(&self) -> Vector2<u32> {
		Vector2::new(self.image.width(), self.image.height())
	}
}
