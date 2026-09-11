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
#[serde(transparent)]
pub struct ProfilePanel {
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

	#[serde(rename = "slide")]
	Slide(ProfilePanelSlideShader),
}

/// Panel shader none
#[derive(Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ProfilePanelNoneShader {
	pub geometries: Vec<ProfilePanelNoneGeometry>,

	#[serde(default)]
	pub background_color: [f32; 4],
}

/// Panel shader none geometry
#[derive(Debug)]
#[serde_with::serde_as]
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum ProfilePanelNoneGeometry {
	Full {
		#[serde_as(as = "serde_with::DisplayFromStr")]
		geometry: Rect<i32, u32>,
	},

	Short(Rect<i32, u32>),
}

/// Panel fade shader
#[derive(Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ProfilePanelFadeShader {
	pub geometries: Vec<ProfilePanelFadeGeometry>,

	pub playlist:      String,
	#[serde(with = "humantime_serde")]
	pub duration:      Duration,
	#[serde(with = "humantime_serde")]
	pub fade_duration: Duration,

	/// Kind
	#[serde(flatten)]
	pub kind: ProfilePanelFadeShaderKind,
}

/// Panel shader fade geometry
#[derive(Debug)]
#[serde_with::serde_as]
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum ProfilePanelFadeGeometry {
	Full {
		#[serde_as(as = "serde_with::DisplayFromStr")]
		geometry: Rect<i32, u32>,
	},

	Short(Rect<i32, u32>),
}

/// Panel fade shader kind
#[derive(Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "fade")]
pub enum ProfilePanelFadeShaderKind {
	#[serde(rename = "basic")]
	Basic,

	#[serde(rename = "out")]
	Out { strength: f32 },
}

/// Panel slide shader
#[derive(Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ProfilePanelSlideShader {
	pub geometries: Vec<ProfilePanelSlideGeometry>,

	pub playlist: String,

	#[serde(with = "humantime_serde")]
	pub duration: Duration,

	pub dir: ProfilePanelSlideDir,

	/// Kind
	#[serde(flatten)]
	pub kind: ProfilePanelSlideShaderKind,
}

/// Panel shader fade geometry
#[derive(Debug)]
#[serde_with::serde_as]
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum ProfilePanelSlideGeometry {
	Full {
		#[serde_as(as = "serde_with::DisplayFromStr")]
		geometry: Rect<i32, u32>,
	},

	Short(Rect<i32, u32>),
}

/// Panel shader slide kind
#[derive(Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "slide")]
pub enum ProfilePanelSlideShaderKind {
	#[serde(rename = "basic")]
	Basic,
}

/// Panel slide direction
#[derive(Clone, Copy, Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProfilePanelSlideDir {
	LeftRight,
	RightLeft,
	UpDown,
	DownUp,
}
