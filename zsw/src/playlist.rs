//! Playlist

// Modules
mod ser;

// Imports
use {
	crate::{
		shared::{AsyncRwLockResource, Locker, LockerStreamExt, PlaylistItemRwLock, PlaylistRwLock, PlaylistsRwLock},
		AppError,
	},
	anyhow::Context,
	async_once_cell::Lazy,
	futures::{Future, StreamExt},
	std::{
		collections::{hash_map, HashMap},
		ffi::OsStr,
		mem,
		path::{Path, PathBuf},
		sync::Arc,
	},
	tokio_stream::wrappers::ReadDirStream,
	zsw_util::PathAppendExt,
};

/// Playlists manager
#[derive(Debug)]
pub struct PlaylistsManager {
	/// Base directory
	base_dir: PathBuf,
}

impl PlaylistsManager {
	/// Loads a playlist
	pub async fn load(
		&self,
		name: &str,
		playlists: &PlaylistsRwLock,
		locker: &mut Locker<'_, 0>,
	) -> Result<Arc<PlaylistRwLock>, AppError> {
		// Check if the playlist is already loaded
		{
			let (playlists, _) = playlists.read(locker).await;
			if let Some(playlist) = playlists.playlists.get(name) {
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
		let (mut playlists, _) = playlists.write(locker).await;
		let playlist = match playlists.playlists.raw_entry_mut().from_key(name) {
			// If it's been inserted to in the meantime, wait on it
			hash_map::RawEntryMut::Occupied(entry) => Arc::clone(entry.get()),

			// Else insert the future to load it
			hash_map::RawEntryMut::Vacant(entry) => {
				// The future to load the playlist
				let load_fut: LoadPlaylistFut = Box::pin({
					let name = name.to_owned();
					let path = self.base_dir.join(&name).with_appended(".yaml");
					async move {
						tracing::debug!(?name, ?path, "Loading playlist");
						match Self::load_raw(path).await {
							Ok(playlist) => {
								tracing::debug!(?name, ?playlist, "Loaded playlist");
								Ok(Arc::new(PlaylistRwLock::new(playlist)))
							},
							Err(err) => {
								tracing::warn!(?name, ?err, "Unable to load playlist");
								Err(Arc::new(err))
							},
						}
					}
				});

				let lazy = Lazy::new(load_fut);
				let (_, playlist) = entry.insert(name.into(), Arc::new(lazy));
				Arc::clone(playlist)
			},
		};

		// Finally wait on the playlist
		mem::drop(playlists);
		playlist
			.get_unpin()
			.await
			.as_ref()
			.map(Arc::clone)
			.map_err(Arc::clone)
			.map_err(AppError::Shared)
	}

	/// Loads all playlists in the playlists directory
	pub async fn load_all_default(
		&self,
		playlists: &PlaylistsRwLock,
		locker: &mut Locker<'_, 0>,
	) -> Result<(), AppError> {
		tokio::fs::read_dir(&self.base_dir)
			.await
			.map(ReadDirStream::new)
			.context("Unable to read playlists directory")?
			.split_locker_async_unordered(locker, async move |entry, mut locker| {
				// Get the name, if it's a yaml file
				let entry = match entry {
					Ok(entry) => entry,
					Err(err) => {
						tracing::warn!(?err, "Unable to read directory entry");
						return;
					},
				};
				let path = entry.path();
				let (Some(name), Some("yaml")) = (path.file_prefix().and_then(OsStr::to_str), path.extension().and_then(OsStr::to_str)) else {
					tracing::debug!(?path, "Ignoring non-playlist file in playlists directory");
					return;
				};

				// Then load it
				let _ = self.load(name, playlists, &mut locker).await;
			})
			.collect::<()>().await;

		Ok(())
	}

	/// Returns all loaded playlists
	pub async fn get_all_loaded(
		&self,
		playlists: &PlaylistsRwLock,
		locker: &mut Locker<'_, 0>,
	) -> Vec<(Arc<str>, Option<Result<Arc<PlaylistRwLock>, AppError>>)> {
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

	/// Loads a playlist
	async fn load_raw(path: PathBuf) -> Result<Playlist, AppError> {
		// Read the file
		let playlist_yaml = tokio::fs::read(&path).await.context("Unable to open file")?;

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
	#[allow(clippy::type_complexity)] // TODO: Refactor the whole type
	playlists: HashMap<Arc<str>, Arc<Lazy<Result<Arc<PlaylistRwLock>, Arc<AppError>>, LoadPlaylistFut>>>,
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
pub async fn create(base_dir: PathBuf) -> Result<(PlaylistsManager, Playlists), AppError> {
	// Create the playlists directory, if it doesn't exist
	tokio::fs::create_dir_all(&base_dir)
		.await
		.context("Unable to create playlists directory")?;

	Ok((PlaylistsManager { base_dir }, Playlists {
		playlists: HashMap::new(),
	}))
}
