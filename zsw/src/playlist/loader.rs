//! Playlist player

// Imports
use {
	super::{Playlist, PlaylistItem, PlaylistItemKind, PlaylistName, ser},
	futures::lock::Mutex,
	std::{collections::HashMap, path::PathBuf, sync::Arc},
	tokio::sync::OnceCell,
	zsw_util::PathAppendExt,
	zutil_app_error::{AppError, Context},
};

/// Playlist loader
#[derive(Debug)]
pub struct PlaylistsLoader {
	/// Playlists directory
	root: PathBuf,

	/// Loaded playlists
	// TODO: Limit the size of this?
	playlists: Mutex<HashMap<PlaylistName, Arc<OnceCell<Arc<Playlist>>>>>,
}

impl PlaylistsLoader {
	/// Creates a new playlists loader
	pub fn new(root: PathBuf) -> Self {
		Self {
			root,
			playlists: Mutex::new(HashMap::new()),
		}
	}

	/// Loads a playlist by name
	pub async fn load(&self, playlist_name: PlaylistName) -> Result<Arc<Playlist>, AppError> {
		self.playlists
			.lock()
			.await
			.entry(playlist_name.clone())
			.or_insert_with(|| Arc::new(OnceCell::new()))
			.get_or_try_init(async move || {
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
				tracing::info!("Loaded playlist {playlist_name:?}");

				Ok(Arc::new(playlist))
			})
			.await
			.map(Arc::clone)
	}

	/// Returns a playlist's path
	pub fn playlist_path(&self, name: &PlaylistName) -> PathBuf {
		self.root.join(&*name.0).with_appended(".toml")
	}
}
