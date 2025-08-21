//! Playlist

// Modules
mod player;
mod ser;

// Exports
pub use self::player::PlaylistPlayer;

// Imports
use {
	crate::AppError,
	std::{
		borrow::Borrow,
		fmt,
		path::{Path, PathBuf},
		sync::Arc,
	},
	zsw_util::PathAppendExt,
	zutil_app_error::Context,
};

/// Playlist loader
#[derive(Debug)]
pub struct PlaylistsLoader {
	/// Playlists directory
	root: PathBuf,
}

impl PlaylistsLoader {
	/// Creates a new playlists loader
	pub fn new(root: PathBuf) -> Self {
		Self { root }
	}

	/// Loads a playlist by name
	pub async fn load(&self, playlist_name: PlaylistName) -> Result<Playlist, AppError> {
		// Try to read the file
		let playlist_path = self.playlist_path(&playlist_name);
		tracing::debug!(%playlist_name, ?playlist_path, "Loading playlist");
		let playlist_toml = tokio::fs::read_to_string(playlist_path)
			.await
			.context("Unable to open file")?;

		// And parse it
		let playlist = toml::from_str::<ser::Playlist>(&playlist_toml).context("Unable to parse playlist")?;
		let playlist = Playlist {
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
		};

		Ok(playlist)
	}

	/// Returns a playlist's path
	pub fn playlist_path(&self, name: &PlaylistName) -> PathBuf {
		self.root.join(&*name.0).with_appended(".toml")
	}
}

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
