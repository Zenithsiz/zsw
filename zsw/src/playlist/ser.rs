//! Serialized playlist

use std::path::PathBuf;

/// Playlist
#[derive(Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Playlist {
	pub items: Vec<PlaylistItem>,
}

/// Playlist item
#[derive(Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct PlaylistItem {
	pub path: PathBuf,

	#[serde(default = "PlaylistItem::default_follow_symlinks")]
	pub follow_symlinks: bool,

	#[serde(default = "PlaylistItem::default_directory_recursive")]
	pub recursive: bool,
}

impl PlaylistItem {
	fn default_follow_symlinks() -> bool {
		true
	}

	fn default_directory_recursive() -> bool {
		true
	}
}
