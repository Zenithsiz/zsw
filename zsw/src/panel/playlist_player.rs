//! Playlist player

// Imports
use {
	crate::playlist::{Playlist, PlaylistItem},
	anyhow::Context,
	async_walkdir::WalkDir,
	futures::{stream::FuturesUnordered, TryStreamExt},
	rand::seq::SliceRandom,
	std::path::{Path, PathBuf},
	tokio_stream::wrappers::ReadDirStream,
};

/// Playlist player
#[derive(Debug)]
pub struct PlaylistPlayer {
	/// All items
	items: Vec<PathBuf>,

	/// Current item
	cur_idx: Option<usize>,
}

impl PlaylistPlayer {
	/// Creates a new player from a playlist
	pub async fn new(playlist: &Playlist) -> Result<Self, anyhow::Error> {
		let mut items = Self::get_playlist_items(playlist)
			.await
			.context("Unable to get all playlist items")?;
		items.shuffle(&mut rand::thread_rng());

		Ok(Self { items, cur_idx: None })
	}

	/// Returns the next image to load
	pub fn next(&mut self) -> Option<&Path> {
		// Advance the playlist
		self.advance();
		let cur_idx = self.cur_idx?;

		// Then get the item
		let item = self.items.get(cur_idx).expect("Current index was invalid");
		Some(item)
	}

	/// Advances the playlist.
	fn advance(&mut self) {
		// The beginning index
		let begin_idx = match self.items.len() {
			0 => None,
			_ => Some(0),
		};

		self.cur_idx = match self.cur_idx {
			// If we had an index, check if the next is valid
			Some(next_idx) => match self.items.get(next_idx + 1).is_some() {
				// If so, use the next index
				true => Some(next_idx + 1),

				// Else go back to the beginning
				false => begin_idx,
			},

			// If we had none, go back to the beginning
			None => begin_idx,
		};
	}

	/// Collects all items of a playlist
	async fn get_playlist_items(playlist: &Playlist) -> Result<Vec<PathBuf>, anyhow::Error> {
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
			.collect();

		Ok(items)
	}
}
