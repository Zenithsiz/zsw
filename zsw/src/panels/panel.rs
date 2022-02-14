//! Panel

// Imports
use crate::Rect;

/// A panel
#[derive(Clone, Copy, Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Panel {
	/// Geometry
	pub geometry: Rect<i32, u32>,

	/// Duration (in frames)
	pub duration: u64,

	/// Fade point (in frames)
	pub fade_point: u64,
}

impl Panel {
	/// Creates a new panel
	#[must_use]
	pub fn new(geometry: Rect<i32, u32>, duration: u64, fade_point: u64) -> Self {
		Self {
			geometry,
			duration,
			fade_point,
		}
	}
}
