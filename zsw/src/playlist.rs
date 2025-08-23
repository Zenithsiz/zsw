//! Playlist

// Modules
mod loader;
mod player;
mod ser;

// Exports
pub use self::{loader::PlaylistsLoader, player::PlaylistPlayer};

// Imports
use std::{borrow::Borrow, fmt, path::Path, sync::Arc};

/// Playlist
#[derive(Debug)]
pub struct Playlist {
	/// All items
	pub items: Vec<PlaylistItem>,
}

/// Playlist item
#[derive(Clone, Debug)]
pub struct PlaylistItem {
	/// Enabled
	pub enabled: bool,

	/// Kind
	pub kind: PlaylistItemKind,
}

/// Playlist item kind
#[derive(Clone, Debug)]
pub enum PlaylistItemKind {
	/// Directory
	Directory {
		path: Arc<Path>,

		recursive: bool,
	},

	/// File
	File { path: Arc<Path> },
}

/// Playlist name
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Debug)]
pub struct PlaylistName(Arc<str>);

impl From<String> for PlaylistName {
	fn from(s: String) -> Self {
		Self(s.into())
	}
}

impl Borrow<str> for PlaylistName {
	fn borrow(&self) -> &str {
		&self.0
	}
}

impl fmt::Display for PlaylistName {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		self.0.fmt(f)
	}
}
