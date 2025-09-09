//! Serialized profile

// TODO: Allow deserializing durations from strings such as "500ms", "5s", "1m2s", etc.

// Imports
use {
	core::time::Duration,
	serde_with::{DurationSecondsWithFrac, serde_as},
};

/// Profile
#[derive(Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Profile {
	pub panels: Vec<ProfilePanel>,
}

/// Profile panel
#[derive(Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ProfilePanel {
	pub name:   String,
	pub shader: ProfilePanelShader,
}

/// Panel shader
#[derive(Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum ProfilePanelShader {
	#[serde(rename = "none")]
	None(ProfilePanelNoneShader),

	#[serde(rename = "fade")]
	Fade(ProfilePanelFadeShader),
}

/// Panel shader none
#[derive(Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ProfilePanelNoneShader {
	#[serde(default)]
	pub background_color: [f32; 4],
}

/// Panel shader fade
#[derive(Debug)]
#[serde_as]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ProfilePanelFadeShader {
	pub playlists: Vec<String>,

	#[serde_as(as = "DurationSecondsWithFrac<f64>")]
	pub duration: Duration,

	#[serde_as(as = "DurationSecondsWithFrac<f64>")]
	pub fade_duration: Duration,

	/// Inner
	#[serde(flatten)]
	pub inner: ProfilePanelFadeShaderInner,
}

/// Panel shader fade inner
#[derive(Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "fade")]
pub enum ProfilePanelFadeShaderInner {
	#[serde(rename = "basic")]
	Basic,

	#[serde(rename = "white")]
	White { strength: f32 },

	#[serde(rename = "out")]
	Out { strength: f32 },

	#[serde(rename = "in")]
	In { strength: f32 },
}
