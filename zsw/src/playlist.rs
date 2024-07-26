//! Playlist

// Modules
mod ser;

// Imports
use {
	crate::AppError,
	anyhow::Context,
	async_once_cell::Lazy,
	futures::{stream::FuturesUnordered, Future, StreamExt},
	std::{
		collections::{hash_map, HashMap},
		mem,
		path::Path,
		pin::Pin,
		sync::Arc,
	},
	tokio::sync::RwLock,
};

/// Playlists manager
#[derive(Debug)]
pub struct PlaylistsManager {}

impl PlaylistsManager {
	/// Loads a playlist by path
	pub async fn load(
		&self,
		path: &Path,
		playlists: &RwLock<Playlists>,
	) -> Result<(Arc<Path>, Arc<RwLock<Playlist>>), AppError> {
		// Canonicalize the path first, so we don't load the same playlist from two paths
		// (e.g., one relative and one absolute)
		let path = path.canonicalize().context("Unable to canonicalize path")?;

		// Check if the playlist is already loaded
		{
			let playlists = playlists.read().await;
			if let Some((playlist_path, playlist)) = playlists.playlists.get_key_value(&*path) {
				let playlist_path = Arc::clone(playlist_path);
				let playlist_lazy = Arc::clone(playlist);
				mem::drop(playlists);
				return Ok((playlist_path, Self::wait_for_playlist_load(&playlist_lazy).await?));
			}
		}

		// Else lock for write and insert the entry
		let (playlist_path, playlist_lazy) = {
			let mut playlists = playlists.write().await;
			let entry = playlists.playlists.raw_entry_mut().from_key(&*path);
			match entry {
				// If it's been inserted to in the meantime, wait on it
				hash_map::RawEntryMut::Occupied(entry) => (Arc::clone(entry.key()), Arc::clone(entry.get())),

				// Else insert the future to load it
				hash_map::RawEntryMut::Vacant(entry) => {
					let playlist_path = <Arc<Path>>::from(path);
					let playlist_lazy = Lazy::new(Self::load_fut(&playlist_path));
					let (_, playlist_lazy) = entry.insert(Arc::clone(&playlist_path), Arc::new(playlist_lazy));
					(playlist_path, Arc::clone(playlist_lazy))
				},
			}
		};

		// Finally wait on the playlist
		Ok((playlist_path, Self::wait_for_playlist_load(&playlist_lazy).await?))
	}

	/// Creates the load playlist future, `LoadPlaylistFut`
	#[expect(clippy::type_complexity)] // TODO: Refactor the whole type
	pub fn load_fut(
		path: &Path,
	) -> Pin<Box<dyn Future<Output = Result<Arc<RwLock<Playlist>>, Arc<AppError>>> + Send + Sync>> {
		Box::pin({
			let path = path.to_owned();
			async move {
				tracing::debug!(?path, "Loading playlist");
				match Self::load_inner(&path).await {
					Ok(playlist) => {
						tracing::debug!(?path, ?playlist, "Loaded playlist");
						Ok(Arc::new(RwLock::new(playlist)))
					},
					Err(err) => {
						tracing::warn!(?path, ?err, "Unable to load playlist");
						Err(Arc::new(err))
					},
				}
			}
		})
	}

	/// Saves a loaded playlist by path.
	///
	/// Waits for loading, if currently loading.
	/// Returns an error if unloaded
	pub async fn save(&self, path: &Path, playlists: &RwLock<Playlists>) -> Result<(), anyhow::Error> {
		// Get the playlist
		let playlist_lazy = {
			let playlists = playlists.read().await;
			let Some(playlist) = playlists.playlists.get(path) else {
				anyhow::bail!("Playlist {path:?} isn't loaded");
			};
			Arc::clone(playlist)
		};

		// Then wait for it to load
		let playlist = Self::wait_for_playlist_load(&playlist_lazy).await?;

		// Finally save it
		let playlist = Self::serialize_playlist(&playlist).await;
		let playlist_yaml = serde_yaml::to_string(&playlist).context("Unable to serialize playlist")?;
		tokio::fs::write(path, playlist_yaml)
			.await
			.context("Unable to write playlist to file")?;

		Ok(())
	}

	/// Reloads a playlist by path.
	///
	/// If it's not loaded, loads it.
	/// If currently loading, cancels the previous loading and loads again.
	pub async fn reload(&self, path: &Path, playlists: &RwLock<Playlists>) -> Result<Arc<RwLock<Playlist>>, AppError> {
		let playlist_lazy = {
			let mut playlists = playlists.write().await;

			let playlist_lazy = Lazy::new(Self::load_fut(path));
			let playlist_lazy = playlists
				.playlists
				.entry(path.into())
				.insert_entry(Arc::new(playlist_lazy))
				.into_mut();
			Arc::clone(playlist_lazy)
		};

		// Finally wait on the playlist
		Self::wait_for_playlist_load(&playlist_lazy).await
	}

	/// Returns a loaded playlist.
	///
	/// No guarantees are made on which playlist is chosen
	pub async fn get_loaded_any(
		&self,
		playlists: &RwLock<Playlists>,
	) -> Option<(Arc<Path>, Option<Result<Arc<RwLock<Playlist>>, AppError>>)> {
		let playlists = playlists.read().await;

		let (path, playlist) = playlists.playlists.iter().next()?;
		let playlist = playlist.try_get().map(|res| {
			res.as_ref()
				.map(Arc::clone)
				.map_err(Arc::clone)
				.map_err(AppError::Shared)
		});


		Some((Arc::clone(path), playlist))
	}

	/// Returns all loaded playlists
	pub async fn get_all_loaded(
		&self,
		playlists: &RwLock<Playlists>,
	) -> Vec<(Arc<Path>, Option<Result<Arc<RwLock<Playlist>>, AppError>>)> {
		let playlists = playlists.read().await;

		playlists
			.playlists
			.iter()
			.map(|(path, playlist)| {
				let playlist = playlist.try_get().map(|res| {
					res.as_ref()
						.map(Arc::clone)
						.map_err(Arc::clone)
						.map_err(AppError::Shared)
				});

				(Arc::clone(path), playlist)
			})
			.collect()
	}

	/// Loads a playlist
	async fn load_inner(path: &Path) -> Result<Playlist, AppError> {
		// Read the file
		tracing::trace!(?path, "Reading playlist file");
		let playlist_yaml = tokio::fs::read_to_string(path).await.context("Unable to open file")?;

		// And parse it
		tracing::trace!(?path, ?playlist_yaml, "Parsing playlist file");
		let playlist = serde_yaml::from_str::<ser::Playlist>(&playlist_yaml).context("Unable to parse playlist")?;
		tracing::trace!(?path, ?playlist, "Parsed playlist file");
		let playlist = Self::deserialize_playlist(playlist);

		Ok(playlist)
	}

	/// Waits for `playlist` to be loaded
	#[expect(clippy::type_complexity)] // TODO: Refactor the whole type
	async fn wait_for_playlist_load(
		playlist: &Lazy<
			Result<Arc<RwLock<Playlist>>, Arc<AppError>>,
			Pin<Box<dyn Future<Output = Result<Arc<RwLock<Playlist>>, Arc<AppError>>> + Send + Sync>>,
		>,
	) -> Result<Arc<RwLock<Playlist>>, AppError> {
		playlist
			.get_unpin()
			.await
			.as_ref()
			.map(Arc::clone)
			.map_err(Arc::clone)
			.map_err(AppError::Shared)
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
}

/// Playlists
pub struct Playlists {
	/// Playlists
	// Note: We keep all playlists loaded due to them being likely small in both size and quantity.
	//       Even a playlist with 10k file entries, with an average path of 200 bytes, would only occupy
	//       ~2 MiB. This is far less than the size of most images we load.
	#[expect(clippy::type_complexity)] // TODO: Refactor the whole type
	playlists: HashMap<
		Arc<Path>,
		Arc<
			Lazy<
				Result<Arc<RwLock<Playlist>>, Arc<AppError>>,
				Pin<Box<dyn Future<Output = Result<Arc<RwLock<Playlist>>, Arc<AppError>>> + Send + Sync>>,
			>,
		>,
	>,
}

impl std::fmt::Debug for Playlists {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_map()
			.entries(self.playlists.iter().map(|(path, playlist)| (path, playlist.try_get())))
			.finish()
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

/// Creates the playlists service
pub fn create() -> (PlaylistsManager, Playlists) {
	let playlists_manager = PlaylistsManager {};
	let playlists = Playlists {
		playlists: HashMap::new(),
	};

	(playlists_manager, playlists)
}
