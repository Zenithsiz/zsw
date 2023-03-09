//! Playlist

// Modules
mod ser;

// Imports
use {
	crate::{
		shared::{PlaylistItemRwLock, PlaylistRwLock},
		AppError,
	},
	anyhow::Context,
	futures::{StreamExt, TryStreamExt},
	std::{
		collections::HashMap,
		ffi::OsStr,
		path::{Path, PathBuf},
		sync::Arc,
	},
	tokio_stream::wrappers::ReadDirStream,
};

/// Playlists manager
#[derive(Debug)]
pub struct PlaylistsManager {
	/// Base directory
	// TODO: Allow "refreshing" the playlists using this base directory
	_base_dir: PathBuf,
}

/// Playlists
#[derive(Debug)]
pub struct Playlists {
	/// Playlists
	// Note: We keep all playlists loaded due to them being likely small in both size and quantity.
	//       Even a playlist with 10k file entries, with an average path of 200 bytes, would only occupy
	//       ~2 MiB. This is far less than the size of most images we load.
	playlists: HashMap<Arc<str>, Arc<PlaylistRwLock>>,
}

impl Playlists {
	/// Retrieves a playlist
	pub fn get(&self, name: &str) -> Option<Arc<PlaylistRwLock>> {
		self.playlists.get(name).map(Arc::clone)
	}

	/// Returns all items
	pub fn get_all(&self) -> Vec<(Arc<str>, Arc<PlaylistRwLock>)> {
		self.playlists
			.iter()
			.map(|(name, playlist)| (Arc::clone(name), Arc::clone(playlist)))
			.collect()
	}
}


/// Playlist
#[derive(Debug)]
pub struct Playlist {
	/// All items
	items: Vec<Arc<PlaylistItemRwLock>>,
}

impl Playlist {
	/// Returns all items
	pub fn items(&self) -> Vec<Arc<PlaylistItemRwLock>> {
		self.items.clone()
	}
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

/// Creates the playlists service
pub async fn create(base_dir: PathBuf) -> Result<(PlaylistsManager, Playlists), AppError> {
	// Create the playlists directory, if it doesn't exist
	tokio::fs::create_dir_all(&base_dir)
		.await
		.context("Unable to create playlists directory")?;

	// Then read all the playlists
	// TODO: Do this in a separate task *after* creation?
	let playlists = tokio::fs::read_dir(&base_dir)
		.await
		.map(ReadDirStream::new)
		.context("Unable to read playlists directory")?
		.then(async move |entry| {
			// Get the name, if it's a yaml file
			let entry = entry.map_err(AppError::Io)?;
			let path = entry.path();
			let (Some(name), Some("yaml")) = (path.file_prefix().and_then(OsStr::to_str), path.extension().and_then(OsStr::to_str)) else {
				return Ok(None);
			};

			// Then read the file
			tracing::debug!(?name, ?path, "Loading playlist");
			let playlist_yaml = tokio::fs::read(&path).await.context("Unable to open file")?;

			// And load it
			let playlist = serde_yaml::from_slice::<ser::Playlist>(&playlist_yaml).context("Unable to parse playlist")?;
			let playlist = Playlist {
				items: playlist.items.into_iter().map(|item| PlaylistItem {
					enabled: item.enabled,
					kind: match item.kind {
						ser::PlaylistItemKind::Directory { path, recursive } => PlaylistItemKind::Directory { path: path.into(), recursive },
						ser::PlaylistItemKind::File { path } => PlaylistItemKind::File { path: path.into() },
					},
				}).map(PlaylistItemRwLock::new).map(Arc::new).collect(),
			};

			Ok::<_, AppError>(Some((name.to_owned().into(), Arc::new(PlaylistRwLock::new(playlist)))))
		})
		.try_collect::<Vec<_>>()
		.await?
		.into_iter()
		.flatten()
		.collect::<HashMap<_, _>>();

	Ok((PlaylistsManager { _base_dir: base_dir }, Playlists { playlists }))
}
