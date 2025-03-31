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
}
