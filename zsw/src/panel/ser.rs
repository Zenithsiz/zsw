//! Panel serialization / deserialization

// Imports
use zsw_util::Rect;

/// Serialized panel
#[derive(Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Panel {
	pub geometries: Vec<PanelGeometry>,
	pub state:      PanelState,
	pub playlist:   String,
}

/// Serialized panel geometry
#[derive(Debug)]
#[serde_with::serde_as]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct PanelGeometry {
	#[serde_as(as = "serde_with::DisplayFromStr")]
	pub geometry: Rect<i32, u32>,
}


/// Serialized panel state
#[derive(Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct PanelState {
	pub duration:   u64,
	pub fade_point: u64,

	// TODO: Move these to a sub-struct?
	#[serde(default = "default_panel_parallax_ratio")]
	pub parallax_ratio:   f32,
	#[serde(default = "default_panel_parallax_exp")]
	pub parallax_exp:     f32,
	#[serde(default = "default_panel_parallax_reverse")]
	pub reverse_parallax: bool,
}

fn default_panel_parallax_ratio() -> f32 {
	0.998
}
fn default_panel_parallax_exp() -> f32 {
	2.0
}
fn default_panel_parallax_reverse() -> bool {
	false
}
