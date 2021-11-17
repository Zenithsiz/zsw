//! Image request

// Imports
use cgmath::Vector2;
use std::path::PathBuf;

/// An Image request
#[derive(Debug)]
pub struct ImageRequest {
	/// Window size
	pub window_size: Vector2<u32>,

	/// Path
	pub path: PathBuf,
}

/// Load response error
#[derive(Debug, thiserror::Error)]
#[error("Unable to load image {path:?}")]
pub struct LoadImageError {
	/// Path that couldn't be loaded
	pub path: PathBuf,
}
