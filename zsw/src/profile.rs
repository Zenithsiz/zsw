//! Profile

mod ser;

use {
	crate::playlist::PlaylistName,
	core::{str::FromStr, time::Duration},
	std::{borrow::Borrow, collections::BTreeMap, fmt, sync::Arc},
	zsw_util::Rect,
};

/// Profiles
pub type Profiles = BTreeMap<ProfileName, Arc<Profile>>;

/// Profile
#[derive(Debug)]
#[derive(serde::Deserialize)]
#[serde(from = "ser::Profile")]
pub struct Profile {
	/// Panels
	pub panels: Vec<ProfilePanel>,
}

/// Profile panel
#[derive(Debug)]
pub struct ProfilePanel {
	pub shader: ProfilePanelShader,
}

/// Profile panel shader
#[derive(Debug)]
pub enum ProfilePanelShader {
	None(ProfilePanelNoneShader),
	Fade(ProfilePanelFadeShader),
	Slide(ProfilePanelSlideShader),
}

/// Profile panel shader none
#[derive(Debug)]
pub struct ProfilePanelNoneShader {
	pub geometries:       Vec<ProfilePanelNoneGeometry>,
	pub background_color: [f32; 4],
}

/// Profile panel shader none geometry
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub struct ProfilePanelNoneGeometry {
	/// Inner geometry
	pub geometry: Rect<i32, u32>,
}

/// Profile panel fade shader
#[derive(Debug)]
pub struct ProfilePanelFadeShader {
	pub geometries:    Vec<ProfilePanelFadeGeometry>,
	pub playlist:      PlaylistName,
	pub duration:      Duration,
	pub fade_duration: Duration,
	pub kind:          ProfilePanelFadeShaderKind,
}

/// Profile panel shader fade geometry
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub struct ProfilePanelFadeGeometry {
	/// Inner geometry
	pub geometry: Rect<i32, u32>,
}

/// Profile panel fade shader kind
#[derive(Debug)]
pub enum ProfilePanelFadeShaderKind {
	Basic,
	Out { strength: f32 },
}

/// Profile slide panel shader
#[derive(Debug)]
pub struct ProfilePanelSlideShader {
	pub geometries: Vec<ProfilePanelSlideGeometry>,
	pub playlist:   PlaylistName,
	pub duration:   Duration,
	pub kind:       ProfilePanelSlideShaderKind,
}

/// Profile panel shader slide geometry
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub struct ProfilePanelSlideGeometry {
	pub geometry: Rect<i32, u32>,
	pub dir:      ProfilePanelSlideDir,
}

/// Profile panel slide shader kind
#[derive(Debug)]
pub enum ProfilePanelSlideShaderKind {
	Basic,
}

/// Profile panel slide direction
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum ProfilePanelSlideDir {
	LeftRight,
	RightLeft,
	UpDown,
	DownUp,
}

impl From<ser::Profile> for Profile {
	fn from(profile: ser::Profile) -> Self {
		Self {
			panels: profile
				.panels
				.into_iter()
				.map(|panel| ProfilePanel {
					shader: match panel.shader {
						ser::ProfilePanelShader::None(shader) => ProfilePanelShader::None(ProfilePanelNoneShader {
							geometries:       shader
								.geometries
								.into_iter()
								.map(|geometry| match geometry {
									ser::ProfilePanelNoneGeometry::Full { geometry } |
									ser::ProfilePanelNoneGeometry::Short(geometry) => ProfilePanelNoneGeometry { geometry },
								})
								.collect(),
							background_color: shader.background_color,
						}),
						ser::ProfilePanelShader::Fade(shader) => ProfilePanelShader::Fade(ProfilePanelFadeShader {
							geometries:    shader
								.geometries
								.into_iter()
								.map(|geometry| match geometry {
									ser::ProfilePanelFadeGeometry::Full { geometry } |
									ser::ProfilePanelFadeGeometry::Short(geometry) => ProfilePanelFadeGeometry { geometry },
								})
								.collect(),
							playlist:      PlaylistName::from_str(&shader.playlist).into_ok(),
							duration:      shader.duration,
							fade_duration: shader.fade_duration,
							kind:          match shader.kind {
								ser::ProfilePanelFadeShaderKind::Basic => ProfilePanelFadeShaderKind::Basic,
								ser::ProfilePanelFadeShaderKind::Out { strength } =>
									ProfilePanelFadeShaderKind::Out { strength },
							},
						}),
						ser::ProfilePanelShader::Slide(shader) => ProfilePanelShader::Slide(ProfilePanelSlideShader {
							geometries: shader
								.geometries
								.into_iter()
								.map(|geometry| match geometry {
									ser::ProfilePanelSlideGeometry::Full { geometry, dir } =>
										ProfilePanelSlideGeometry {
											geometry,
											dir: match dir {
												ser::ProfilePanelSlideDir::LeftRight => ProfilePanelSlideDir::LeftRight,
												ser::ProfilePanelSlideDir::RightLeft => ProfilePanelSlideDir::RightLeft,
												ser::ProfilePanelSlideDir::UpDown => ProfilePanelSlideDir::UpDown,
												ser::ProfilePanelSlideDir::DownUp => ProfilePanelSlideDir::DownUp,
											},
										},
									ser::ProfilePanelSlideGeometry::Short(geometry) => ProfilePanelSlideGeometry {
										geometry,
										// TODO: Is this a fine default?
										dir: match geometry.width() > geometry.height() {
											true => ProfilePanelSlideDir::LeftRight,
											false => ProfilePanelSlideDir::UpDown,
										},
									},
								})
								.collect(),
							playlist:   PlaylistName::from_str(&shader.playlist).into_ok(),
							duration:   shader.duration,
							kind:       match shader.kind {
								ser::ProfilePanelSlideShaderKind::Basic => ProfilePanelSlideShaderKind::Basic,
							},
						}),
					},
				})
				.collect(),
		}
	}
}

/// Profile name
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub struct ProfileName(Arc<str>);

impl FromStr for ProfileName {
	type Err = !;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(Self(Arc::from(s)))
	}
}

impl AsRef<str> for ProfileName {
	fn as_ref(&self) -> &str {
		&self.0
	}
}

impl Borrow<str> for ProfileName {
	fn borrow(&self) -> &str {
		&self.0
	}
}

impl fmt::Display for ProfileName {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		self.0.fmt(f)
	}
}

impl fmt::Debug for ProfileName {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		self.0.fmt(f)
	}
}
