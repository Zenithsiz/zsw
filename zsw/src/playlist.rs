//! Playlist

// Modules
mod ser;

// Imports
use {
	crate::{
		shared::{AsyncLocker, AsyncRwLockResource, PlaylistItemRwLock, PlaylistRwLock, PlaylistsRwLock},
		AppError,
	},
	anyhow::Context,
	async_once_cell::Lazy,
	futures::Future,
	std::{
		collections::{hash_map, HashMap},
		mem,
		path::Path,
		sync::Arc,
	},
};

/// Playlists manager
#[derive(Debug)]
pub struct PlaylistsManager {}

impl PlaylistsManager {
	/// Loads a playlist by path
	pub async fn load(
		&self,
		path: &Path,
		playlists: &PlaylistsRwLock,
		locker: &mut AsyncLocker<'_, 0>,
	) -> Result<Arc<PlaylistRwLock>, AppError> {
		// Check if the playlist is already loaded
		{
			let (playlists, _) = playlists.read(locker).await;
			if let Some(playlist) = playlists.playlists.get(path) {
				let playlist = Arc::clone(playlist);
				mem::drop(playlists);
				return playlist
					.get_unpin()
					.await
					.as_ref()
					.map(Arc::clone)
					.map_err(Arc::clone)
					.map_err(AppError::Shared);
			}
		}

		// Else lock for write and insert the entry
		let playlist_lazy = {
			let (mut playlists, _) = playlists.write(locker).await;
			let entry = playlists.playlists.raw_entry_mut().from_key(path);
			match entry {
				// If it's been inserted to in the meantime, wait on it
				hash_map::RawEntryMut::Occupied(entry) => Arc::clone(entry.get()),

				// Else insert the future to load it
				hash_map::RawEntryMut::Vacant(entry) => {
					let playlist_lazy = Lazy::new(Self::load_fut(path));
					let (_, playlist_lazy) = entry.insert(path.into(), Arc::new(playlist_lazy));
					Arc::clone(playlist_lazy)
				},
			}
		};

		// Finally wait on the playlist
		playlist_lazy
			.get_unpin()
			.await
			.as_ref()
			.map(Arc::clone)
			.map_err(Arc::clone)
			.map_err(AppError::Shared)
	}

	/// Reloads a playlist by path.
	///
	/// If it's not loaded, loads it.
	/// If currently loading, cancels the previous loading and loads again.
	pub async fn reload(
		&self,
		path: &Path,
		playlists: &PlaylistsRwLock,
		locker: &mut AsyncLocker<'_, 0>,
	) -> Result<Arc<PlaylistRwLock>, AppError> {
		let playlist_lazy = {
			let (mut playlists, _) = playlists.write(locker).await;

			let playlist_lazy = Lazy::new(Self::load_fut(path));
			let playlist_lazy = playlists
				.playlists
				.entry(path.into())
				.insert_entry(Arc::new(playlist_lazy))
				.into_mut();
			Arc::clone(playlist_lazy)
		};

		// Finally wait on the playlist
		playlist_lazy
			.get_unpin()
			.await
			.as_ref()
			.map(Arc::clone)
			.map_err(Arc::clone)
			.map_err(AppError::Shared)
	}

	/// Returns all loaded playlists
	pub async fn get_all_loaded(
		&self,
		playlists: &PlaylistsRwLock,
		locker: &mut AsyncLocker<'_, 0>,
	) -> Vec<(Arc<Path>, Option<Result<Arc<PlaylistRwLock>, AppError>>)> {
		let (playlists, _) = playlists.read(locker).await;

		playlists
			.playlists
			.iter()
			.map(|(name, playlist)| {
				let playlist = match playlist.try_get() {
					Some(res) => match res {
						Ok(playlist) => Some(Ok(Arc::clone(playlist))),
						Err(err) => Some(Err(AppError::Shared(Arc::clone(err)))),
					},
					None => None,
				};

				(Arc::clone(name), playlist)
			})
			.collect()
	}

	/// Creates the load playlist future, `LoadPlaylistFut`
	fn load_fut(path: &Path) -> LoadPlaylistFut {
		Box::pin({
			let path = path.to_owned();
			async move {
				tracing::debug!(?path, "Loading playlist");
				match Self::load_inner(&path).await {
					Ok(playlist) => {
						tracing::debug!(?path, ?playlist, "Loaded playlist");
						Ok(Arc::new(PlaylistRwLock::new(playlist)))
					},
					Err(err) => {
						tracing::warn!(?path, ?err, "Unable to load playlist");
						Err(Arc::new(err))
					},
				}
			}
		})
	}

	/// Loads a playlist
	async fn load_inner(path: &Path) -> Result<Playlist, AppError> {
		// Read the file
		let playlist_yaml = tokio::fs::read(path).await.context("Unable to open file")?;

		// And parse it
		let playlist = serde_yaml::from_slice::<ser::Playlist>(&playlist_yaml).context("Unable to parse playlist")?;
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
				.map(PlaylistItemRwLock::new)
				.map(Arc::new)
				.collect(),
		};

		Ok(playlist)
	}
}

/// Playlists
pub struct Playlists {
	/// Playlists
	// Note: We keep all playlists loaded due to them being likely small in both size and quantity.
	//       Even a playlist with 10k file entries, with an average path of 200 bytes, would only occupy
	//       ~2 MiB. This is far less than the size of most images we load.
	#[expect(clippy::type_complexity)] // TODO: Refactor the whole type
	playlists: HashMap<Arc<Path>, Arc<Lazy<Result<Arc<PlaylistRwLock>, Arc<AppError>>, LoadPlaylistFut>>>,
}

impl std::fmt::Debug for Playlists {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_map()
			.entries(self.playlists.iter().map(|(name, playlist)| (name, playlist.try_get())))
			.finish()
	}
}

/// Future that loads playlists
type LoadPlaylistFut = impl Future<Output = Result<Arc<PlaylistRwLock>, Arc<AppError>>> + Send + Sync + Unpin;

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
pub fn create() -> (PlaylistsManager, Playlists) {
	let playlists_manager = PlaylistsManager {};
	let playlists = Playlists {
		playlists: HashMap::new(),
	};

	(playlists_manager, playlists)
}
