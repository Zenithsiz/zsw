//! Panel

// Imports
use zsw_util::Rect;

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

	/// Parallax scale, 0.0 .. 1.0
	#[serde(default = "default_parallax_ratio")]
	pub parallax_ratio: f32,
}

impl Panel {
	/// Creates a new panel
	#[must_use]
	pub fn new(geometry: Rect<i32, u32>, duration: u64, fade_point: u64, parallax_ratio: f32) -> Self {
		Self {
			geometry,
			duration,
			fade_point,
			parallax_ratio,
		}
	}
}

fn default_parallax_ratio() -> f32 {
	1.0
}
