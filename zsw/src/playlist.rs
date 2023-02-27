//! Playlist

// Imports
use {
	anyhow::Context,
	futures::{StreamExt, TryStreamExt},
	std::{
		collections::HashMap,
		ffi::OsStr,
		path::PathBuf,
		sync::{Arc, RwLock},
	},
	tokio_stream::wrappers::ReadDirStream,
};

/// Playlists manager
#[derive(Debug)]
pub struct PlaylistsManager {
	/// Base directory
	// TODO: Allow "refreshing" the playlists using this base directory
	_base_dir: PathBuf,

	/// Playlists
	// Note: We keep all playlists loaded due to them being likely small in both size and quantity.
	//       Even a playlist with 10k file entries, with an average path of 200 bytes, would only occupy
	//       ~2 MiB. This is far less than the size of most images we load.
	playlists: RwLock<HashMap<Arc<str>, Arc<Playlist>>>,
}

impl PlaylistsManager {
	/// Creates a playlist manager
	pub async fn new(base_dir: PathBuf) -> Result<Self, anyhow::Error> {
		// Create the playlists directory, if it doesn't exist
		tokio::fs::create_dir_all(&base_dir)
			.await
			.context("Unable to create playlists directory")?;

		// Then read all the playlists
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
				let playlist = serde_yaml::from_slice(&playlist_yaml).context("Unable to parse playlist")?;

				Ok::<_, anyhow::Error>(Some((name.to_owned(), playlist)))
			})
			.try_collect::<Vec<_>>()
			.await?
			.into_iter()
			.flatten()
			.map(|(name, playlist)| (name.into(), Arc::new(playlist)))
			.collect::<HashMap<_, _>>();

		Ok(Self {
			_base_dir: base_dir,
			playlists: RwLock::new(playlists),
		})
	}

	/// Retrieves a playlist
	#[expect(clippy::disallowed_methods)] // DEADLOCK: We don't lock anything else after the lock
	pub fn get(&self, name: &str) -> Option<Arc<Playlist>> {
		let playlists = self.playlists.read().expect("Poisoned");
		let playlist = playlists.get(name)?;
		Some(Arc::clone(playlist))
	}

	/// Retrieves all playlists
	#[expect(clippy::disallowed_methods)] // DEADLOCK: We don't lock anything else after the lock
	pub fn get_all(&self) -> Vec<(Arc<str>, Arc<Playlist>)> {
		let playlists = self.playlists.read().expect("Poisoned");
		playlists
			.iter()
			.map(|(name, playlist)| (Arc::clone(name), Arc::clone(playlist)))
			.collect()
	}
}


/// Playlist
#[derive(Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
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
}

/// Playlist item
#[derive(Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum PlaylistItem {
	/// Directory
	Directory {
		path: PathBuf,

		#[serde(default = "PlaylistItem::default_directory_recursive")]
		recursive: bool,
	},

	/// File
	File { path: PathBuf },
}

impl PlaylistItem {
	fn default_directory_recursive() -> bool {
		true
	}
}
