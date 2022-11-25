//! Image handling

// Features
#![feature(never_type)]

// Modules
pub mod loader;

// Exports
pub use loader::{ImageLoader, ImageReceiver, RawImage, RawImageProvider};

// Imports
use {cgmath::Vector2, image::DynamicImage};

/// Loaded image
#[derive(Debug)]
pub struct Image<P: RawImageProvider> {
	/// Image name
	pub name: String,

	/// Image
	pub image: DynamicImage,

	/// Raw image token
	pub raw_image_token: <P::RawImage as RawImage>::Token,
}

impl<P: RawImageProvider> Image<P> {
	/// Returns the image's size
	#[must_use]
	pub fn size(&self) -> Vector2<u32> {
		Vector2::new(self.image.width(), self.image.height())
	}
}
