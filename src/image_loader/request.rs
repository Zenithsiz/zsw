//! Image request

// Imports
use super::paths;
use cgmath::Vector2;
use std::path::PathBuf;

/// A request for an image
#[derive(Debug)]
pub struct ImageRequest {
	/// Image size
	pub size: Vector2<u32>,

	/// Path
	pub path: PathBuf,

	/// Index
	pub idx: paths::RecvIdx,
}

/// Response error
#[derive(Debug, thiserror::Error)]
#[error("Unable to load image")]
pub struct ResponseError {
	/// Index to remove
	pub(super) idx: paths::RecvIdx,
}
