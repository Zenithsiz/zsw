//! Profile

// Imports
use {std::path::PathBuf, zsw_util::Rect};

/// A profile
#[derive(Clone, Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Profile {
	/// Root path
	pub root_path: PathBuf,

	/// All panels
	pub panels: Vec<Panel>,

	/// Max image size
	#[serde(default)]
	pub max_image_size: Option<u32>,
}

/// Profile panel
#[derive(Clone, Copy, Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Panel {
	/// Geometry
	pub geometry: Rect<i32, u32>,

	/// Duration (in frames)
	pub duration: u64,

	/// Fade point (in frames)
	pub fade_point: u64,

	/// Parallax
	#[serde(default)]
	pub parallax: PanelParallax,
}

/// Panel parallax
#[derive(Clone, Copy, Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct PanelParallax {
	/// Parallax scale, 0.0 .. 1.0
	#[serde(default = "zsw_util::default_panel_parallax_ratio")]
	pub ratio: f32,

	/// Parallax exponentiation
	#[serde(default = "zsw_util::default_panel_parallax_exp")]
	pub exp: f32,

	/// Reverse parallax
	#[serde(default = "zsw_util::default_panel_parallax_reverse")]
	pub reverse: bool,
}

impl Default for PanelParallax {
	fn default() -> Self {
		Self {
			ratio:   zsw_util::default_panel_parallax_ratio(),
			exp:     zsw_util::default_panel_parallax_exp(),
			reverse: zsw_util::default_panel_parallax_reverse(),
		}
	}
}
