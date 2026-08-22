//! Playlist

// Modules
mod player;
mod ser;

// Exports
pub use self::player::PlaylistPlayer;

// Imports
use {
	core::str::FromStr,
	std::{borrow::Borrow, collections::BTreeMap, fmt, path::Path, sync::Arc},
};

/// Playlists
pub type Playlists = BTreeMap<PlaylistName, Playlist>;

/// Playlist
#[derive(Debug)]
#[derive(serde::Deserialize)]
#[serde(from = "ser::Playlist")]
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
		path:            Arc<Path>,
		follow_symlinks: bool,
		recursive:       bool,
	},

	/// File
	File { path: Arc<Path> },
}

impl From<ser::Playlist> for Playlist {
	fn from(playlist: ser::Playlist) -> Self {
		Self {
			items: playlist
				.items
				.into_iter()
				.map(|item| PlaylistItem {
					enabled: item.enabled,
					kind:    match item.kind {
						ser::PlaylistItemKind::Directory {
							path,
							follow_symlinks,
							recursive,
						} => PlaylistItemKind::Directory {
							path: path.into(),
							follow_symlinks,
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

impl FromStr for PlaylistName {
	type Err = !;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(Self(Arc::from(s)))
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
