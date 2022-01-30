//! Panels profiles

// Imports
use {crate::Rect, std::time::Duration};

/// A panels profile
#[derive(Clone, Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct PanelsProfile {
	/// All panels
	panels: Vec<PanelsProfile>,
}

/// A panel profile
#[derive(Clone, Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
struct PanelProfile {
	/// Geometry
	pub geometry: Rect<u32>,

	/// Progress
	pub progress: f32,

	/// Image duration
	pub image_duration: Duration,

	/// Fade point
	pub fade_point: f32,
}
