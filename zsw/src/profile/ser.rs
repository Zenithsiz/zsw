//! Serialized profile

// Imports
use zsw_util::DurationDisplay;

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
	pub display: String,
	pub shader:  ProfilePanelShader,
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

	#[serde(rename = "slide")]
	Slide(ProfilePanelSlideShader),
}

/// Panel shader none
#[derive(Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ProfilePanelNoneShader {
	#[serde(default)]
	pub background_color: [f32; 4],
}

/// Panel fade shader
#[derive(Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ProfilePanelFadeShader {
	pub playlists:     Vec<String>,
	pub duration:      DurationDisplay,
	pub fade_duration: DurationDisplay,

	/// Inner
	#[serde(flatten)]
	pub inner: ProfilePanelFadeShaderInner,
}

/// Panel fade shader inner
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

/// Panel slide shader
#[derive(Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ProfilePanelSlideShader {
	/// Inner
	#[serde(flatten)]
	pub inner: ProfilePanelSlideShaderInner,
}

/// Panel shader slide inner
#[derive(Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "slide")]
pub enum ProfilePanelSlideShaderInner {
	#[serde(rename = "basic")]
	Basic,
}
