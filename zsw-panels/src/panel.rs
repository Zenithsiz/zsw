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

	// TODO: make parallax optional with a struct wrapped in `Option`
	
	/// Parallax scale, 0.0 .. 1.0
	#[serde(default = "default_parallax_ratio")]
	pub parallax_ratio: f32,
	
	/// Reverse parallax
	#[serde(default = "default_reverse_parallax")]
	pub reverse_parallax: bool,
}

impl Panel {
	/// Creates a new panel
	#[must_use]
	pub fn new(geometry: Rect<i32, u32>, duration: u64, fade_point: u64) -> Self {
		Self {
			geometry,
			duration,
			fade_point,
			parallax_ratio: self::default_parallax_ratio(),
			reverse_parallax: self::default_reverse_parallax(),
		}
	}
}

fn default_parallax_ratio() -> f32 {
	1.0
}

fn default_reverse_parallax() -> bool {
	false
}
