//! Playlist player

// Imports
use {
	crate::{
		playlist::PlaylistItemKind,
		shared::{AsyncLocker, AsyncRwLockResource, LockerIteratorExt, PlaylistRwLock},
		AppError,
	},
	anyhow::Context,
	async_walkdir::WalkDir,
	futures::TryStreamExt,
	rand::{rngs::StdRng, seq::SliceRandom, SeedableRng},
	std::{collections::HashSet, path::Path, sync::Arc},
	tokio_stream::wrappers::ReadDirStream,
};

/// Playlist player
#[derive(Debug)]
pub struct PlaylistPlayer {
	/// All items
	items: HashSet<Arc<Path>>,

	/// Previous items
	///
	/// Last item is newest item
	prev_items: Vec<Arc<Path>>,

	/// Next items
	///
	/// Last item is next item
	next_items: Vec<Arc<Path>>,

	/// Rng
	rng: StdRng,
}

impl PlaylistPlayer {
	/// Creates a new player from a playlist
	pub async fn new(playlist: &PlaylistRwLock, locker: &mut AsyncLocker<'_, 0>) -> Result<Self, AppError> {
		let items = Self::get_playlist_items(playlist, locker)
			.await
			.context("Unable to get all playlist items")?;

		Ok(Self {
			items,
			prev_items: vec![],
			next_items: vec![],
			rng: StdRng::from_entropy(),
		})
	}

	/// Removes an item from the playlist
	pub fn remove(&mut self, path: &Path) {
		// Remove the item from all our playlists
		// TODO: Not have `O(N)` complexity on prev / next items
		let _ = self.items.remove(path);
		let _ = self.prev_items.drain_filter(|item| &**item == path);
		let _ = self.next_items.drain_filter(|item| &**item == path);
	}

	/// Returns an iterator over all items in the playlist
	pub fn all_items(&self) -> impl Iterator<Item = &Arc<Path>> + ExactSizeIterator {
		self.items.iter()
	}

	/// Returns an iterator over all consumed items
	///
	/// They are ordered from newest to oldest
	pub fn prev_items(&self) -> impl Iterator<Item = &Arc<Path>> + ExactSizeIterator {
		self.prev_items.iter().rev()
	}

	/// Returns an iterator that peeks over the remaining items in this loop.
	///
	/// They are ordered from next to last
	pub fn peek_next_items(&self) -> impl Iterator<Item = &Arc<Path>> + ExactSizeIterator {
		self.next_items.iter().rev()
	}

	/// Returns the next image to load
	pub fn next(&mut self) -> Option<Arc<Path>> {
		// If we're out of current items, shuffle the items in
		// Note: If we don't actually have any items, this is essentially a no-op
		if self.next_items.is_empty() {
			self.next_items.extend(self.items.iter().cloned());
			self.next_items.shuffle(&mut self.rng);
		}

		// Then pop the last item
		let item = self.next_items.pop()?;
		self.prev_items.push(Arc::clone(&item));
		Some(item)
	}

	/// Collects all items of a playlist
	async fn get_playlist_items(
		playlist: &PlaylistRwLock,
		locker: &mut AsyncLocker<'_, 0>,
	) -> Result<HashSet<Arc<Path>>, AppError> {
		let items = playlist.read(locker).await.0.items();

		// TODO: Give them some order here? At least for non-directory kinds
		let items = items
			.into_iter()
			.split_locker_async_unordered(locker, |item, mut locker| async move {
				let item = item.read(&mut locker).await.0.clone();

				// If not enabled, don't return any items
				if !item.enabled {
					tracing::trace!(?item, "Ignoring non-enabled playlist item");
					return Ok(vec![]);
				}

				// Else check the kind of item
				let item = match item.kind {
					PlaylistItemKind::Directory { ref path, recursive } => match recursive {
						true => WalkDir::new(path)
							.filter(async move |entry| match entry.file_type().await.map(|ty| ty.is_dir()) {
								Err(_) | Ok(true) => async_walkdir::Filtering::Ignore,
								Ok(false) => async_walkdir::Filtering::Continue,
							})
							.map_err(anyhow::Error::new)
							.and_then(async move |entry| {
								let path = entry.path();
								tokio::fs::canonicalize(path)
									.await
									.context("Unable to canonicalize path")
							})
							.try_collect()
							.await
							.context("Unable to recursively read directory files")?,
						false => {
							let dir = tokio::fs::read_dir(path).await.context("Unable to read directory")?;
							ReadDirStream::new(dir)
								.map_err(anyhow::Error::new)
								.and_then(async move |entry| {
									tokio::fs::canonicalize(entry.path())
										.await
										.context("Unable to canonicalize path")
								})
								.try_collect()
								.await
								.context("Unable to recursively read directory files")?
						},
					},
					PlaylistItemKind::File { ref path } => vec![tokio::fs::canonicalize(path)
						.await
						.context("Unable to canonicalize path")?],
				};

				Ok::<_, AppError>(item)
			})
			.try_collect::<Vec<_>>()
			.await
			.context("Unable to collect all items")?
			.into_iter()
			.flatten()
			.map(Into::into)
			.collect();

		Ok(items)
	}
}
