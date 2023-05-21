//! Serialized playlist

// Imports
use std::{collections::HashMap, path::PathBuf};

/// Playlists
#[derive(Debug)]
pub struct Playlists {
	pub playlists: HashMap<String, Playlist>,
}

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
	/// Enabled
	#[serde(default = "PlaylistItem::default_enabled")]
	pub enabled: bool,

	/// Kind
	#[serde(flatten)]
	pub kind: PlaylistItemKind,
}

impl PlaylistItem {
	fn default_enabled() -> bool {
		true
	}
}

/// Playlist item kind
#[derive(Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum PlaylistItemKind {
	/// Directory
	Directory {
		path: PathBuf,

		#[serde(default = "PlaylistItemKind::default_directory_recursive")]
		recursive: bool,
	},

	/// File
	File { path: PathBuf },
}

impl PlaylistItemKind {
	fn default_directory_recursive() -> bool {
		true
	}
}
