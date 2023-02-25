//! Playlist player

// Imports
use {
	crate::playlist::{Playlist, PlaylistItem},
	anyhow::Context,
	async_walkdir::WalkDir,
	futures::{stream::FuturesUnordered, TryStreamExt},
	rand::{rngs::StdRng, seq::SliceRandom, SeedableRng},
	std::{collections::HashSet, path::Path, sync::Arc},
	tokio_stream::wrappers::ReadDirStream,
};

/// Playlist player
#[derive(Debug)]
pub struct PlaylistPlayer {
	/// All items
	items: HashSet<Arc<Path>>,

	/// Current items
	cur_items: Vec<Arc<Path>>,

	/// Rng
	rng: StdRng,
}

impl PlaylistPlayer {
	/// Creates a new player from a playlist
	pub async fn new(playlist: &Playlist) -> Result<Self, anyhow::Error> {
		let items = Self::get_playlist_items(playlist)
			.await
			.context("Unable to get all playlist items")?;
		//

		Ok(Self {
			items,
			cur_items: vec![],
			rng: StdRng::from_entropy(),
		})
	}

	/// Removes an item from the playlist
	pub fn remove(&mut self, path: &Path) {
		// We don't care about the removed item
		let _ = self.items.remove(path);
	}

	/// Returns an iterator over all items, including consumed ones
	pub fn all_items(&self) -> impl Iterator<Item = &Arc<Path>> + ExactSizeIterator {
		self.items.iter()
	}

	/// Returns an iterator that peeks over the remaining items in this loop.
	///
	/// They are ordered from next to last
	pub fn peek_next_items(&self) -> impl Iterator<Item = &Arc<Path>> + ExactSizeIterator {
		self.cur_items.iter().rev()
	}

	/// Returns the next image to load
	pub fn next(&mut self) -> Option<Arc<Path>> {
		// If we're out of current items, shuffle the items in
		// Note: If we don't actually have any items, this is essentially a no-op
		if self.cur_items.is_empty() {
			self.cur_items.extend(self.items.iter().cloned());
			self.cur_items.shuffle(&mut self.rng);
		}

		// Then pop the last item
		self.cur_items.pop()
	}

	/// Collects all items of a playlist
	async fn get_playlist_items(playlist: &Playlist) -> Result<HashSet<Arc<Path>>, anyhow::Error> {
		let items = playlist
			.items()
			.iter()
			.map(|item| async move {
				let item = match *item {
					PlaylistItem::Directory { ref path, recursive } => match recursive {
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
					PlaylistItem::File { ref path } => vec![tokio::fs::canonicalize(path)
						.await
						.context("Unable to canonicalize path")?],
				};

				Ok::<_, anyhow::Error>(item)
			})
			.collect::<FuturesUnordered<_>>()
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
