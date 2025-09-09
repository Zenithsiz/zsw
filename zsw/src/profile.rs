//! Profile

// Modules
mod profiles;
pub mod ser;

// Exports
pub use self::profiles::Profiles;

// Imports
use {
	crate::{panel::PanelName, playlist::PlaylistName},
	core::time::Duration,
	std::{borrow::Borrow, fmt, sync::Arc},
};

/// Profile
#[derive(Debug)]
pub struct Profile {
	/// Panels
	pub panels: Vec<ProfilePanel>,
}

/// Profile panel
#[derive(Debug)]
pub struct ProfilePanel {
	pub name:   PanelName,
	pub state:  ProfilePanelState,
	pub shader: ProfilePanelShader,
}

/// Profile panel state
#[derive(Debug)]
pub struct ProfilePanelState {
	pub duration:      Duration,
	pub fade_duration: Duration,
}

/// Profile panel shader
#[derive(Debug)]
pub enum ProfilePanelShader {
	None(ProfilePanelNoneShader),
	Fade(ProfilePanelFadeShader),
}

/// Profile panel shader none
#[derive(Debug)]
pub struct ProfilePanelNoneShader {
	pub background_color: [f32; 4],
}

/// Profile panel shader fade
#[derive(Debug)]
pub struct ProfilePanelFadeShader {
	pub playlists: Vec<PlaylistName>,
	pub inner:     ProfilePanelFadeShaderInner,
}

/// Profile panel shader fade inner
#[derive(Debug)]
pub enum ProfilePanelFadeShaderInner {
	Basic,
	White { strength: f32 },
	Out { strength: f32 },
	In { strength: f32 },
}


/// Profile name
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub struct ProfileName(Arc<str>);

impl From<String> for ProfileName {
	fn from(s: String) -> Self {
		Self(s.into())
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
