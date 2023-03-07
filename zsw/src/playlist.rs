//! Playlist

// Modules
mod ser;

// Imports
use {
	anyhow::Context,
	futures::{StreamExt, TryStreamExt},
	std::{collections::HashMap, ffi::OsStr, path::PathBuf},
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
	playlists: HashMap<String, Playlist>,
}

impl Playlists {
	/// Retrieves a playlist
	pub fn get(&self, name: &str) -> Option<&Playlist> {
		self.playlists.get(name)
	}

	/// Returns an iterator over all playlists mutably
	pub fn get_all_mut(&mut self) -> impl Iterator<Item = (&str, &mut Playlist)> {
		self.playlists
			.iter_mut()
			.map(|(name, playlist)| (name.as_str(), playlist))
	}
}


/// Playlist
#[derive(Debug)]
pub struct Playlist {
	/// All items
	items: Vec<PlaylistItem>,
}

impl Playlist {
	/// Creates a new playlist
	pub fn _new() -> Self {
		Self { items: vec![] }
	}

	/// Adds an item to this playlist
	pub fn _add(&mut self, item: PlaylistItem) {
		self.items.push(item);
	}

	/// Returns all items
	pub fn items(&self) -> &[PlaylistItem] {
		self.items.as_ref()
	}

	/// Returns all items mutably
	pub fn items_mut(&mut self) -> &mut [PlaylistItem] {
		self.items.as_mut()
	}
}

/// Playlist item
#[derive(Debug)]
pub struct PlaylistItem {
	/// Enabled
	pub enabled: bool,

	/// Kind
	pub kind: PlaylistItemKind,
}

/// Playlist item kind
#[derive(Debug)]
pub enum PlaylistItemKind {
	/// Directory
	Directory {
		path: PathBuf,

		recursive: bool,
	},

	/// File
	File { path: PathBuf },
}

/// Creates the playlists service
pub async fn create(base_dir: PathBuf) -> Result<(PlaylistsManager, Playlists), anyhow::Error> {
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
			let entry = entry?;
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
						ser::PlaylistItemKind::Directory { path, recursive } => PlaylistItemKind::Directory { path, recursive },
						ser::PlaylistItemKind::File { path } => PlaylistItemKind::File { path },
					},
				}).collect(),
			};

			Ok::<_, anyhow::Error>(Some((name.to_owned(), playlist)))
		})
		.try_collect::<Vec<_>>()
		.await?
		.into_iter()
		.flatten()
		.collect::<HashMap<_, _>>();

	Ok((PlaylistsManager { _base_dir: base_dir }, Playlists { playlists }))
}
