//! Image request

// Imports
use cgmath::Vector2;

/// A request to process an image
#[derive(Clone, Copy, Debug)]
pub struct ImageRequest {
	/// Window size
	///
	/// The window size the image will be processed in.
	pub window_size: Vector2<u32>,
}
