//! Profile

// Modules
mod ser;

// Imports
use {
	crate::{display::DisplayName, playlist::PlaylistName},
	core::time::Duration,
	std::{borrow::Borrow, fmt, sync::Arc},
	zsw_util::Resources,
};

/// Profiles
pub type Profiles = Resources<ProfileName, Arc<Profile>>;

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
	pub display_name: DisplayName,
	pub shader:       ProfilePanelShader,
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
	pub background_color: [f32; 4],
}

/// Profile panel fade shader
#[derive(Debug)]
pub struct ProfilePanelFadeShader {
	pub playlists:     Vec<PlaylistName>,
	pub duration:      Duration,
	pub fade_duration: Duration,
	pub inner:         ProfilePanelFadeShaderInner,
}

/// Profile panel fade shader inner
#[derive(Debug)]
pub enum ProfilePanelFadeShaderInner {
	Basic,
	White { strength: f32 },
	Out { strength: f32 },
	In { strength: f32 },
}

/// Profile slide panel shader
#[derive(Debug)]
pub struct ProfilePanelSlideShader {
	pub inner: ProfilePanelSlideShaderInner,
}

/// Profile panel slide shader inner
#[derive(Debug)]
pub enum ProfilePanelSlideShaderInner {
	Basic,
}

impl From<ser::Profile> for Profile {
	fn from(profile: ser::Profile) -> Self {
		Self {
			panels: profile
				.panels
				.into_iter()
				.map(|panel| ProfilePanel {
					display_name: DisplayName::from(panel.display),
					shader:       match panel.shader {
						ser::ProfilePanelShader::None(shader) => ProfilePanelShader::None(ProfilePanelNoneShader {
							background_color: shader.background_color,
						}),
						ser::ProfilePanelShader::Fade(shader) => ProfilePanelShader::Fade(ProfilePanelFadeShader {
							playlists:     shader.playlists.into_iter().map(PlaylistName::from).collect(),
							duration:      shader.duration,
							fade_duration: shader.fade_duration,
							inner:         match shader.inner {
								ser::ProfilePanelFadeShaderInner::Basic => ProfilePanelFadeShaderInner::Basic,
								ser::ProfilePanelFadeShaderInner::White { strength } =>
									ProfilePanelFadeShaderInner::White { strength },
								ser::ProfilePanelFadeShaderInner::Out { strength } =>
									ProfilePanelFadeShaderInner::Out { strength },
								ser::ProfilePanelFadeShaderInner::In { strength } =>
									ProfilePanelFadeShaderInner::In { strength },
							},
						}),
						ser::ProfilePanelShader::Slide(shader) => ProfilePanelShader::Slide(ProfilePanelSlideShader {
							inner: match shader.inner {
								ser::ProfilePanelSlideShaderInner::Basic => ProfilePanelSlideShaderInner::Basic,
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

impl From<String> for ProfileName {
	fn from(s: String) -> Self {
		Self(s.into())
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
