//! Playlist

// Modules
mod player;
mod ser;

// Exports
pub use self::player::PlaylistPlayer;

// Imports
use {
	crate::AppError,
	anyhow::{anyhow, Context},
	futures::{stream::FuturesUnordered, StreamExt},
	std::{
		borrow::Borrow,
		collections::HashMap,
		ffi::OsStr,
		fmt,
		path::{Path, PathBuf},
		sync::Arc,
	},
	tokio::sync::RwLock,
	tokio_stream::wrappers::ReadDirStream,
	zsw_util::PathAppendExt,
};

/// Playlists
#[derive(Debug)]
pub struct Playlists {
	/// Playlists directory
	root: PathBuf,

	/// Playlists
	playlists: HashMap<PlaylistName, Arc<RwLock<Playlist>>>,
}

impl Playlists {
	/// Loads all playlists from a directory
	pub async fn load(root: PathBuf) -> Result<Self, anyhow::Error> {
		tokio::fs::create_dir_all(&root)
			.await
			.context("Unable to create playlists directory")?;

		let playlists = tokio::fs::read_dir(&root)
			.await
			.map(ReadDirStream::new)
			.context("Unable to read playlists directory")?
			.then(|entry| async {
				let res: Result<_, anyhow::Error> = try {
					// Ignore directories and non `.toml` files
					let entry = entry.context("Unable to get entry")?;
					let entry_path = entry.path();
					if entry
						.file_type()
						.await
						.context("Unable to get entry metadata")?
						.is_dir() || entry_path.extension().and_then(OsStr::to_str) != Some("toml")
					{
						return None;
					}

					// Then get the playlist name from the file
					let playlist_name = entry_path
						.file_stem()
						.context("Entry path had no file stem")?
						.to_os_string()
						.into_string()
						.map(PlaylistName::from)
						.map_err(|file_name| anyhow!("Entry name was non-utf8: {file_name:?}"))?;

					let playlist = self::load_playlist(&entry_path)
						.await
						.with_context(|| format!("Unable to load playlist: {entry_path:?}"))?;
					tracing::debug!(?playlist_name, ?playlist, "Loaded playlist");

					(playlist_name, Arc::new(RwLock::new(playlist)))
				};

				res.inspect_err(|err| tracing::warn!(?err, "Unable to load entry")).ok()
			})
			.filter_map(|res| async move { res })
			.collect()
			.await;

		let playlists = Self { root, playlists };


		Ok(playlists)
	}

	/// Gets a playlist
	pub fn get(&self, name: &PlaylistName) -> Option<Arc<RwLock<Playlist>>> {
		let playlist = self.playlists.get(name)?;
		Some(Arc::clone(playlist))
	}

	/// Gets an arbitrary playlist
	pub fn _get_any(&self) -> Option<(PlaylistName, Arc<RwLock<Playlist>>)> {
		self.playlists
			.iter()
			.next()
			.map(|(name, playlist)| (name.clone(), Arc::clone(playlist)))
	}

	/// Gets all playlists
	pub fn get_all(&self) -> Vec<(PlaylistName, Arc<RwLock<Playlist>>)> {
		self.playlists
			.iter()
			.map(|(name, playlist)| (name.clone(), Arc::clone(playlist)))
			.collect()
	}

	/// Returns a playlist's path
	pub fn playlist_path(&self, name: &PlaylistName) -> PathBuf {
		self.root.join(&*name.0).with_appended(".toml")
	}

	/// Adds a playlist.
	///
	/// Saves the playlist to disk.
	pub async fn add(&mut self, path: &Path) -> Result<(PlaylistName, Arc<RwLock<Playlist>>), anyhow::Error> {
		// Create the playlist path, ensuring we don't overwrite an existing path
		let mut playlist_name = path
			.file_name()
			.context("Path has no file name")?
			.to_os_string()
			.into_string()
			.map_err(|file_name| anyhow!("Playlist file name was non-utf8: {file_name:?}"))?;
		while self.playlists.contains_key(playlist_name.as_str()) {
			playlist_name.push_str("-new");
		}
		let playlist_name = PlaylistName::from(playlist_name);

		// Load the playlist
		let playlist = self::load_playlist(path).await?;
		let playlist = self
			.playlists
			.entry(playlist_name.clone())
			.insert_entry(Arc::new(RwLock::new(playlist)))
			.into_mut();
		let playlist = Arc::clone(playlist);

		// Save it to disk
		let ser_playlist = self::serialize_playlist(&playlist).await;
		let playlist_toml = toml::to_string(&ser_playlist).context("Unable to serialize playlist")?;
		let playlist_path = self.playlist_path(&playlist_name);
		tokio::fs::write(playlist_path, playlist_toml)
			.await
			.context("Unable to write playlist to file")?;

		Ok((playlist_name, playlist))
	}

	/// Saves a loaded playlist by name.
	///
	/// If the playlist doesn't exist, returns `Err`.
	pub async fn save(&self, name: &PlaylistName) -> Result<(), anyhow::Error> {
		// Get the playlist
		let playlist = match self.playlists.get(name) {
			Some(playlist) => Arc::clone(playlist),
			None => anyhow::bail!("Playlist {name:?} isn't loaded"),
		};

		// And save it
		let playlist = self::serialize_playlist(&playlist).await;
		let playlist_toml = toml::to_string(&playlist).context("Unable to serialize playlist")?;
		let playlist_path = self.playlist_path(name);
		tokio::fs::write(playlist_path, playlist_toml)
			.await
			.context("Unable to write playlist to file")?;

		Ok(())
	}

	/// Reloads a playlist by name.
	pub async fn reload(&mut self, name: PlaylistName) -> Result<Arc<RwLock<Playlist>>, AppError> {
		let playlist_path = self.playlist_path(&name);
		let playlist = self::load_playlist(&playlist_path).await?;
		let playlist = self
			.playlists
			.entry(name)
			.insert_entry(Arc::new(RwLock::new(playlist)))
			.into_mut();

		Ok(Arc::clone(playlist))
	}
}

/// Playlist
#[derive(Debug)]
pub struct Playlist {
	/// All items
	items: Vec<Arc<RwLock<PlaylistItem>>>,
}

impl Playlist {
	/// Returns all items
	pub fn items(&self) -> Vec<Arc<RwLock<PlaylistItem>>> {
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

/// Loads a playlist
async fn load_playlist(path: &Path) -> Result<Playlist, AppError> {
	// Read the file
	tracing::trace!(?path, "Reading playlist file");
	let playlist_toml = tokio::fs::read_to_string(path).await.context("Unable to open file")?;

	// And parse it
	tracing::trace!(?path, ?playlist_toml, "Parsing playlist file");
	let playlist = toml::from_str::<ser::Playlist>(&playlist_toml).context("Unable to parse playlist")?;
	tracing::trace!(?path, ?playlist, "Parsed playlist file");
	let playlist = self::deserialize_playlist(playlist);

	Ok(playlist)
}

/// Serializes a playlist to it's serialized format
async fn serialize_playlist(playlist: &RwLock<Playlist>) -> ser::Playlist {
	ser::Playlist {
		items: {
			let playlist = playlist.read().await;
			playlist
				.items
				.iter()
				.map(|item| async move {
					let item = item.read().await;
					ser::PlaylistItem {
						enabled: item.enabled,
						kind:    match &item.kind {
							PlaylistItemKind::Directory { path, recursive } => ser::PlaylistItemKind::Directory {
								path:      path.to_path_buf(),
								recursive: *recursive,
							},
							PlaylistItemKind::File { path } => ser::PlaylistItemKind::File {
								path: path.to_path_buf(),
							},
						},
					}
				})
				.collect::<FuturesUnordered<_>>()
				.collect()
				.await
		},
	}
}

/// Deserializes a playlist from it's serialized format
fn deserialize_playlist(playlist: ser::Playlist) -> Playlist {
	Playlist {
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
			.map(RwLock::new)
			.map(Arc::new)
			.collect(),
	}
}
