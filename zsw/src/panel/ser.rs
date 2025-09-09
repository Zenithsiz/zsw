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
	pub shader: PanelShader,
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
pub enum PanelShader {
	#[serde(rename = "none")]
	None {
		#[serde(default)]
		background_color: [f32; 4],
	},

	#[serde(rename = "fade")]
	Fade(PanelFadeShader),
}


/// Configuration shader fade inner
#[derive(Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct PanelFadeShader {
	/// Inner
	#[serde(flatten)]
	pub inner: PanelFadeShaderInner,
}

/// Configuration shader fade inner
#[derive(Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "fade")]
pub enum PanelFadeShaderInner {
	#[serde(rename = "basic")]
	Basic,

	#[serde(rename = "white")]
	White { strength: f32 },

	#[serde(rename = "out")]
	Out { strength: f32 },

	#[serde(rename = "in")]
	In { strength: f32 },
}
