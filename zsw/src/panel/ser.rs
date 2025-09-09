//! Panel serialization / deserialization

// Imports
use {serde_with::serde_as, zsw_util::Rect};

/// Serialized panel
#[derive(Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Panel {
	pub geometries: Vec<PanelGeometry>,
}

/// Serialized panel geometry
#[derive(Debug)]
#[serde_with::serde_as]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct PanelGeometry {
	#[serde_as(as = "serde_with::DisplayFromStr")]
	pub geometry: Rect<i32, u32>,
}
