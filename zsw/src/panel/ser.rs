//! Panel serialization / deserialization

// Imports
use {
	core::time::Duration,
	serde_with::{DurationSecondsWithFrac, serde_as},
	zsw_util::Rect,
};

/// Serialized panel
#[derive(Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Panel {
	pub geometries: Vec<PanelGeometry>,
	pub state:      PanelState,

	/// Shader
	#[serde(default)]
	pub shader: Option<PanelShader>,
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
// TODO: Instead allow deserializing from strings such as "500ms", "5s", "1m2s", etc.
#[derive(Debug)]
#[serde_as]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct PanelState {
	#[serde_as(as = "DurationSecondsWithFrac<f64>")]
	pub duration: Duration,

	#[serde_as(as = "DurationSecondsWithFrac<f64>")]
	pub fade_duration: Duration,
}

/// Configuration shader
#[derive(Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
#[expect(variant_size_differences, reason = "16 bytes is still reasonable for this type")]
pub enum PanelShader {
	#[serde(rename = "none")]
	None {
		#[serde(default)]
		background_color: [f32; 4],
	},

	#[serde(rename = "fade")]
	Fade,

	#[serde(rename = "fade-white")]
	FadeWhite { strength: f32 },

	#[serde(rename = "fade-out")]
	FadeOut { strength: f32 },

	#[serde(rename = "fade-in")]
	FadeIn { strength: f32 },
}
