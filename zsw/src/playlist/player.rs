//! Playlist player

// Imports
use {
	super::{Playlist, PlaylistItemKind},
	futures::{StreamExt, stream::FuturesUnordered},
	rand::{SeedableRng, rngs::StdRng, seq::SliceRandom},
	std::{
		collections::{HashSet, VecDeque},
		path::Path,
		sync::Arc,
	},
	tokio::{fs, sync::Mutex},
	zsw_util::{UnwrapOrReturnExt, WalkDir},
	zutil_app_error::AppError,
};

/// Playlist player
#[derive(Debug)]
pub struct PlaylistPlayer {
	/// All items
	all_items: HashSet<Arc<Path>>,

	/// Current items
	cur_items: VecDeque<Arc<Path>>,

	/// Number of old items to keep
	max_old_items: usize,

	/// Current item position (within `cur_items`)
	cur_pos: usize,

	/// Rng
	rng: StdRng,
}

impl PlaylistPlayer {
	/// Creates a new player from a playlist
	pub async fn new(playlist: &Playlist) -> Self {
		let all_items = Mutex::new(HashSet::new());

		playlist
			.items
			.iter()
			.map(async |item| {
				// If not enabled, skip it
				if !item.enabled {
					return;
				}

				// Else check the kind of item
				match item.kind {
					PlaylistItemKind::Directory {
						path: ref dir_path,
						recursive,
					} =>
						WalkDir::builder()
							.max_depth(match recursive {
								true => None,
								false => Some(0),
							})
							.recurse_symlink(true)
							.build(dir_path.to_path_buf())
							.map(|entry| async {
								let entry = match entry {
									Ok(entry) => entry,
									Err(err) => {
										let err = AppError::<()>::new(&err);
										tracing::warn!("Unable to read directory entry: {}", err.pretty());
										return;
									},
								};

								let path = entry.path();
								if fs::metadata(&path)
									.await
									.map_err(|err| {
										let err = AppError::<()>::new(&err);
										tracing::warn!(
											"Unable to get playlist entry {path:?} metadata: {}",
											err.pretty()
										);
									})
									.unwrap_or_return()?
									.is_dir()
								{
									// If it's a directory, skip it
									return;
								}

								match tokio::fs::canonicalize(&path).await {
									Ok(entry) => _ = all_items.lock().await.insert(entry.into()),
									Err(err) => {
										let err = AppError::<()>::new(&err);
										tracing::warn!("Unable to read playlist entry {path:?}: {}", err.pretty());
									},
								}
							})
							.collect::<FuturesUnordered<_>>()
							.await
							.collect::<()>()
							.await,

					PlaylistItemKind::File { ref path } => match tokio::fs::canonicalize(path).await {
						Ok(path) => _ = all_items.lock().await.insert(path.into()),
						Err(err) => {
							let err = AppError::<()>::new(&err);
							tracing::warn!("Unable to canonicalize playlist entry {path:?}: {}", err.pretty());
						},
					},
				}
			})
			.collect::<FuturesUnordered<_>>()
			.collect::<()>()
			.await;

		Self {
			all_items:     all_items.into_inner(),
			cur_items:     VecDeque::new(),
			max_old_items: 100,
			cur_pos:       0,
			rng:           StdRng::from_os_rng(),
		}
	}

	/// Returns the previous position in the playlist
	pub fn prev_pos(&self) -> Option<usize> {
		self.cur_pos.checked_sub(1)
	}

	/// Returns the current position in the playlist
	pub fn cur_pos(&self) -> usize {
		self.cur_pos
	}

	/// Returns the next position in the playlist
	pub fn next_pos(&self) -> usize {
		self.cur_pos.checked_add(1).expect("Playlist position overflowed")
	}

	/// Returns the number of items until a shuffle is necessary
	pub fn remaining_until_shuffle(&self) -> usize {
		self.cur_items.len().saturating_sub(self.cur_pos)
	}

	/// Removes an item from the playlist
	pub fn remove(&mut self, path: &Path) {
		// TODO: Do we care if the path didn't exist?
		_ = self.all_items.remove(path);

		// Remove all matches from the current items, adjusting the indexes along the way
		let mut cur_idx = 0;
		self.cur_items.retain(|item| {
			// If this isn't the item, go next
			if &**item != path {
				cur_idx += 1;
				return true;
			}

			// Else if this is the item, and our current position
			// is after it, adjust the current position
			// Note: Since we're removing the item, we don't increase `cur_idx`.
			if self.cur_pos > cur_idx {
				self.cur_pos -= 1;
			}

			false
		});
	}

	/// Returns an iterator over all items in the playlist
	pub fn all_items(&self) -> impl ExactSizeIterator<Item = &Arc<Path>> {
		self.all_items.iter()
	}

	/// Steps the player backwards.
	///
	/// Returns `Err(())` if there is no previous item.
	pub fn step_prev(&mut self) -> Result<(), ()> {
		// If we're empty or at the start, we can't retract
		if self.all_items.is_empty() || self.cur_pos == 0 {
			return Err(());
		}

		// Otherwise, just go back
		self.cur_pos -= 1;

		Ok(())
	}

	/// Steps the player forward
	pub fn step_next(&mut self) {
		// If we're at the end, refill
		if self.remaining_until_shuffle() == 0 {
			self.refill();
		}

		// Otherwise, just go to the next item
		self.cur_pos += 1;
	}

	/// Refills the playlist items
	///
	/// Does not move the image that's currently selected,
	/// but may change the value of the current position.
	fn refill(&mut self) {
		// If we're empty, we can't fill
		if self.all_items.is_empty() {
			return;
		}

		// Shuffle in all the new items
		let mut new_items = self.all_items.iter().map(Arc::clone).collect::<Vec<_>>();
		new_items.shuffle(&mut self.rng);
		self.cur_pos = self.cur_items.len();
		self.cur_items.extend(new_items);

		// And drop any old items from the back
		if let Some(old_items) = self.cur_pos.checked_sub(self.max_old_items) {
			_ = self.cur_items.drain(..old_items);
			self.cur_pos -= old_items;
		}
	}

	/// Returns the previous image to load.
	///
	/// Returns `None` if we're at the start of the playlist
	pub fn prev(&self) -> Option<Arc<Path>> {
		let item = self.cur_items.get(self.prev_pos()?)?;
		let item = Arc::clone(item);

		Some(item)
	}

	/// Returns the current image to load.
	///
	/// Returns `None` when the playlist is empty,
	/// otherwise gets the current image
	pub fn cur(&mut self) -> Option<Arc<Path>> {
		// If we don't have a current image, refill
		if self.remaining_until_shuffle() == 0 {
			self.refill();
		}

		let item = self.cur_items.get(self.cur_pos())?;
		let item = Arc::clone(item);

		Some(item)
	}

	/// Returns the next image to load
	///
	/// Returns `None` when the playlist is empty,
	/// otherwise gets the next image
	pub fn next(&mut self) -> Option<Arc<Path>> {
		// If we don't have a next image, refill
		if self.remaining_until_shuffle() <= 1 {
			self.refill();
		}

		let item = self.cur_items.get(self.next_pos())?;
		let item = Arc::clone(item);

		Some(item)
	}
}
