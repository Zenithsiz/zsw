//! Image handling

// Features
#![feature(never_type)]

// Modules
pub mod service;

// Exports
pub use service::{ImageLoader, ImageReceiver};

// Imports
use {cgmath::Vector2, image::DynamicImage};

/// Loaded image
#[derive(Debug)]
pub struct Image {
	/// Image name
	pub name: String,

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
