//! Serialized profile

use {core::time::Duration, zsw_util::Rect};

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
	pub geometries: Vec<PanelGeometry>,
	pub shader:     ProfilePanelShader,
}

/// Panel geometry
#[derive(Debug)]
#[serde_with::serde_as]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct PanelGeometry {
	#[serde_as(as = "serde_with::DisplayFromStr")]
	pub geometry: Rect<i32, u32>,
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
	pub playlist:      String,
	#[serde(with = "humantime_serde")]
	pub duration:      Duration,
	#[serde(with = "humantime_serde")]
	pub fade_duration: Duration,

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

	#[serde(rename = "out")]
	Out { strength: f32 },
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
