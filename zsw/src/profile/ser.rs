//! Serialized profile

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
	pub panel:     String,
	pub playlists: Vec<String>,
}
