//! Playlist player

// Imports
use {
	super::Playlist,
	core::clone::Share,
	rand::{rngs::StdRng, seq::SliceRandom},
	std::{
		collections::{HashSet, VecDeque},
		io,
		path::Path,
		sync::Arc,
	},
	zsw_util::{AppError, WalkDir},
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
	/// Creates a new, empty, player
	pub fn new(playlist: &Playlist) -> Result<Self, AppError> {
		let all_items = self::load_playlist_items(playlist)?;

		Ok(Self {
			all_items,
			cur_items: VecDeque::new(),
			max_old_items: 100,
			cur_pos: 0,
			rng: rand::make_rng(),
		})
	}

	/// Returns the current position in the playlist
	pub fn cur_pos(&self) -> usize {
		self.cur_pos
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

	/// Steps the player backwards.
	///
	/// Returns `Err(())` if there is no previous item.
	pub fn step_prev(&mut self) -> Result<(), ()> {
		// If we're empty or at the start, we can't retract
		if self.all_items.is_empty() || self.cur_pos == 0 {
			return Err(());
		}

		self.cur_pos -= 1;
		Ok(())
	}

	/// Steps the player forward
	pub fn step_next(&mut self) {
		// If we're at the end, refill
		if self.remaining_until_shuffle() == 0 {
			self.refill();
		}

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
		let mut new_items = self.all_items.iter().map(Share::share).collect::<Vec<_>>();
		new_items.shuffle(&mut self.rng);
		self.cur_items.extend(new_items);

		// And drop any old items from the back
		if let Some(old_items) = self.cur_pos.checked_sub(self.max_old_items) {
			_ = self.cur_items.drain(..old_items);
			self.cur_pos -= old_items;
		}
	}

	/// Gets an item in this player by index.
	///
	/// Returns it's absolute index.
	///
	/// The index `0` is the current item, with negative indices
	/// corresponding to past items, and positive indices to future
	/// items.
	pub fn get(&mut self, rel_idx: isize) -> Option<(usize, Arc<Path>)> {
		// If the index being asked for is positive and doesn't exist yet, try to refill
		if let Ok(rel_idx) = usize::try_from(rel_idx) &&
			self.remaining_until_shuffle() <= rel_idx
		{
			self.refill();
		}

		let idx = self.cur_pos.checked_add_signed(rel_idx)?;
		let item = self.cur_items.get(idx)?;

		Some((idx, item.share()))
	}
}

/// Loads all entries in a playlist
// TODO: Cache this in the playlist itself
fn load_playlist_items(playlist: &Playlist) -> Result<HashSet<Arc<Path>>, AppError> {
	let mut items = HashSet::new();
	for item in &playlist.items {
		let builder = WalkDir::builder()
			.recurse_symlink(item.follow_symlinks)
			.max_depth(match item.recursive {
				true => None,
				false => Some(1),
			});

		let dir = match builder.build(item.path.as_path()) {
			Ok(dir) => dir,
			// If it wasn't a directory, just add it
			Err(err) if err.kind() == io::ErrorKind::NotADirectory => {
				_ = items.insert(item.path.share());
				continue;
			},
			// Otherwise, this is a fatal error
			Err(err) => {
				let err = AppError::new(&err);
				tracing::warn!("Unable to read directory {:?}: {err:?}", item.path);
				continue;
			},
		};

		for entry in dir {
			let entry = match entry {
				Ok(entry) => entry,
				Err(err) => {
					let err = AppError::new(&err);
					tracing::warn!("Unable to read directory entry: {err:?}");
					continue;
				},
			};

			let file_type = match entry.file_type() {
				Ok(file_type) => file_type,
				Err(err) => {
					let err = AppError::new(&err);
					tracing::warn!("Unable to read directory entry file: {err:?}");
					continue;
				},
			};

			if !file_type.is_dir() {
				_ = items.insert(entry.path().into());
			}
		}
	}

	Ok(items)
}
