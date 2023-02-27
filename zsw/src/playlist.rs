//! Playlist

// Imports
use {
	anyhow::Context,
	std::{
		collections::HashMap,
		path::{Path, PathBuf},
		sync::Arc,
	},
	zsw_util::PathAppendExt,
};

/// Playlist manager
#[derive(Debug)]
pub struct PlaylistManager {
	/// Base Directory
	base_dir: PathBuf,

	/// Cached playlists
	// TODO: Not lock whole map while loading a playlist?
	// TODO: Enforce a max size on this by only allowing X playlists?
	cached_playlists: async_lock::RwLock<HashMap<String, Arc<Playlist>>>,
}

impl PlaylistManager {
	/// Creates a playlist manager
	pub fn new(base_dir: PathBuf) -> Self {
		Self {
			base_dir,
			cached_playlists: async_lock::RwLock::new(HashMap::new()),
		}
	}

	/// Retrieves a playlist
	#[allow(clippy::disallowed_methods)] // We only lock it temporarily
	pub async fn get(&self, name: &str) -> Result<Arc<Playlist>, anyhow::Error> {
		// Check if it's cached with a read-only lock
		let cached_playlists = self.cached_playlists.upgradable_read().await;
		if let Some(playlist) = cached_playlists.get(name) {
			return Ok(Arc::clone(playlist));
		}

		// Else upgrade and, before loading it, check again if it was loaded in the meantime
		let mut cached_playlists = async_lock::RwLockUpgradableReadGuard::upgrade(cached_playlists).await;
		if let Some(playlist) = cached_playlists.get(name) {
			return Ok(Arc::clone(playlist));
		}

		// Else load it
		let playlist = Self::load(&self.base_dir, name)
			.await
			.with_context(|| format!("Unable to load playlist {name:?}"))?;

		let playlist = cached_playlists
			.entry(name.to_owned())
			.insert_entry(Arc::new(playlist))
			.into_mut();

		Ok(Arc::clone(playlist))
	}

	/// Loads a playlist
	async fn load(base_dir: &Path, name: &str) -> Result<Playlist, anyhow::Error> {
		// Try to read the file
		let path = base_dir.join(name).with_appended(".yaml");
		tracing::debug!(?name, ?path, "Loading playlist");
		let playlist_yaml = tokio::fs::read(path).await.context("Unable to open file")?;

		// Then load it
		let playlist = serde_yaml::from_slice(&playlist_yaml).context("Unable to parse playlist")?;
		Ok(playlist)
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
