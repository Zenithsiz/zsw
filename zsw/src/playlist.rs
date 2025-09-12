//! Playlist

// Modules
mod player;
mod ser;

// Exports
pub use self::player::PlaylistPlayer;

// Imports
use {
	std::{borrow::Borrow, fmt, path::Path, sync::Arc},
	zsw_util::{ResourceManager, resource_manager},
};

/// Playlists
pub type Playlists = ResourceManager<PlaylistName, Playlist, ser::Playlist>;

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

impl resource_manager::FromSerialized<PlaylistName, ser::Playlist> for Playlist {
	fn from_serialized(_name: PlaylistName, playlist: ser::Playlist) -> Self {
		Self {
			items: playlist
				.items
				.into_iter()
				.map(|item| PlaylistItem {
					enabled: item.enabled,
					kind:    match item.kind {
						ser::PlaylistItemKind::Directory { path, recursive } => PlaylistItemKind::Directory {
							path: path.into(),
							recursive,
						},
						ser::PlaylistItemKind::File { path } => PlaylistItemKind::File { path: path.into() },
					},
				})
				.collect(),
		}
	}
}

/// Playlist name
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub struct PlaylistName(Arc<str>);

impl From<String> for PlaylistName {
	fn from(s: String) -> Self {
		Self(s.into())
	}
}

impl AsRef<str> for PlaylistName {
	fn as_ref(&self) -> &str {
		&self.0
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

impl fmt::Debug for PlaylistName {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		self.0.fmt(f)
	}
}
