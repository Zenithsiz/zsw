//! Image handling


// Modules
mod loader;
mod uvs;

// Exports
pub use {loader::ImageLoader, uvs::ImageUvs};

// Imports
use {
	cgmath::Vector2,
	image::{DynamicImage, GenericImageView},
	std::{path::PathBuf, sync::Arc},
};

/// Loaded image
#[derive(Debug)]
pub struct Image {
	/// Path of the image
	pub path: Arc<PathBuf>,

	/// Image
	pub image: DynamicImage,
}

impl Image {
	/// Returns the image's size
	pub fn size(&self) -> Vector2<u32> {
		Vector2::new(self.image.width(), self.image.height())
	}
}
