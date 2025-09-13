//! Profile

// Modules
mod ser;

// Imports
use {
	crate::{display::DisplayName, playlist::PlaylistName},
	core::time::Duration,
	std::{borrow::Borrow, fmt, sync::Arc},
	zsw_util::{DurationDisplay, ResourceManager, resource_manager},
};

/// Profiles
pub type Profiles = ResourceManager<ProfileName, Profile, ser::Profile>;

/// Profile
#[derive(Debug)]
pub struct Profile {
	/// Name
	pub name: ProfileName,

	/// Panels
	pub panels: Vec<ProfilePanel>,
}

/// Profile panel
#[derive(Debug)]
pub struct ProfilePanel {
	pub display: DisplayName,
	pub shader:  ProfilePanelShader,
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

impl resource_manager::FromSerialized<ProfileName, ser::Profile> for Profile {
	fn from_serialized(name: ProfileName, profile: ser::Profile) -> Self {
		Self {
			name,
			panels: profile
				.panels
				.into_iter()
				.map(|panel| ProfilePanel {
					display: DisplayName::from(panel.display),
					shader:  match panel.shader {
						ser::ProfilePanelShader::None(shader) => ProfilePanelShader::None(ProfilePanelNoneShader {
							background_color: shader.background_color,
						}),
						ser::ProfilePanelShader::Fade(shader) => ProfilePanelShader::Fade(ProfilePanelFadeShader {
							playlists:     shader.playlists.into_iter().map(PlaylistName::from).collect(),
							duration:      shader.duration.0,
							fade_duration: shader.fade_duration.0,
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

impl resource_manager::ToSerialized<ProfileName, ser::Profile> for Profile {
	fn to_serialized(&self, _name: &ProfileName) -> ser::Profile {
		ser::Profile {
			panels: self
				.panels
				.iter()
				.map(|panel| ser::ProfilePanel {
					display: panel.display.to_string(),
					shader:  match &panel.shader {
						ProfilePanelShader::None(shader) =>
							ser::ProfilePanelShader::None(ser::ProfilePanelNoneShader {
								background_color: shader.background_color,
							}),
						ProfilePanelShader::Fade(shader) =>
							ser::ProfilePanelShader::Fade(ser::ProfilePanelFadeShader {
								playlists:     shader.playlists.iter().map(PlaylistName::to_string).collect(),
								duration:      DurationDisplay(shader.duration),
								fade_duration: DurationDisplay(shader.fade_duration),
								inner:         match shader.inner {
									ProfilePanelFadeShaderInner::Basic => ser::ProfilePanelFadeShaderInner::Basic,
									ProfilePanelFadeShaderInner::White { strength } =>
										ser::ProfilePanelFadeShaderInner::White { strength },
									ProfilePanelFadeShaderInner::Out { strength } =>
										ser::ProfilePanelFadeShaderInner::Out { strength },
									ProfilePanelFadeShaderInner::In { strength } =>
										ser::ProfilePanelFadeShaderInner::In { strength },
								},
							}),
						ProfilePanelShader::Slide(shader) =>
							ser::ProfilePanelShader::Slide(ser::ProfilePanelSlideShader {
								inner: match shader.inner {
									ProfilePanelSlideShaderInner::Basic => ser::ProfilePanelSlideShaderInner::Basic,
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
