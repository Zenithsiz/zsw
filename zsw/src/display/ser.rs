//! Serialized display

// Imports
use {serde_with::serde_as, zsw_util::Rect};

/// Serialized display
#[derive(Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Display {
	pub geometries: Vec<DisplayGeometry>,
}

/// Serialized display geometry
#[derive(Debug)]
#[serde_with::serde_as]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct DisplayGeometry {
	#[serde_as(as = "serde_with::DisplayFromStr")]
	pub geometry: Rect<i32, u32>,
}
