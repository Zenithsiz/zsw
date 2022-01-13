//! A geometry

// Imports
use crate::{gl_image::GlImage, Rect};
use std::time::Instant;

/// Geometry state
#[derive(Debug)]
pub struct Geometry {
	/// Geometry
	pub geometry: Rect<u32>,

	/// Image
	pub image: GeometryImage,

	/// Progress
	pub progress: f32,
}

/// Image of the geometry
#[derive(Debug)]
pub enum GeometryImage {
	/// Empty
	///
	/// This means that no images have been assigned to this geometry yet.
	Empty,

	/// Primary only
	///
	/// The primary image is loaded. The back image is still not available
	PrimaryOnly(GlImage),

	/// Both
	///
	/// Both images are loaded to be faded in between
	Both {
		/// Current image
		cur: GlImage,

		/// Next
		next: GlImage,
	},

	/// Swapped
	///
	/// Front and back images have been swapped, and the next image needs
	/// to be loaded
	Swapped {
		/// Previous image
		prev: GlImage,

		/// Current image
		cur: GlImage,

		/// Instant we were swapped
		since: Instant,
	},
}
