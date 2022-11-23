//! Panel

// Imports
use zsw_util::Rect;

/// A panel
#[derive(Clone, Copy, Debug)]
pub struct Panel {
	/// Geometry
	pub geometry: Rect<i32, u32>,

	/// Duration (in frames)
	pub duration: u64,

	/// Fade point (in frames)
	pub fade_point: u64,

	// TODO: make parallax optional with a struct wrapped in `Option`
	/// Parallax scale, 0.0 .. 1.0
	pub parallax_ratio: f32,

	/// Parallax exponentiation
	pub parallax_exp: f32,

	/// Reverse parallax
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
			parallax_ratio: zsw_util::default_panel_parallax_ratio(),
			parallax_exp: zsw_util::default_panel_parallax_exp(),
			reverse_parallax: zsw_util::default_panel_parallax_reverse(),
		}
	}
}
