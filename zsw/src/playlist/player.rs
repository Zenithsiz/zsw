//! Playlist player

// Imports
use {
	rand::{rngs::StdRng, seq::SliceRandom, SeedableRng},
	std::{
		collections::{HashSet, VecDeque},
		path::Path,
		sync::Arc,
	},
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
	pub fn new() -> Self {
		Self {
			all_items:     HashSet::new(),
			cur_items:     VecDeque::new(),
			max_old_items: 100,
			cur_pos:       0,
			rng:           StdRng::from_entropy(),
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

	/// Adds an item to the playlist
	pub fn add(&mut self, path: Arc<Path>) {
		// TODO: Should we care if the item was already in?
		_ = self.all_items.insert(path);
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

	/// Removes all paths from the playlist
	pub fn remove_all(&mut self) {
		self.all_items.clear();
		self.cur_items.clear();
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
		// If we're empty, we can't advance
		if self.all_items.is_empty() {
			return;
		}

		// If we're at the end
		if self.cur_pos >= self.cur_items.len() {
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

			return;
		}

		// Otherwise, just go to the next item
		self.cur_pos += 1;
	}

	/// Returns the previous image to load
	pub fn prev(&self) -> Option<Arc<Path>> {
		let item = self.cur_items.get(self.prev_pos()?)?;
		let item = Arc::clone(item);

		Some(item)
	}

	/// Returns the current image to load
	pub fn cur(&self) -> Option<Arc<Path>> {
		let item = self.cur_items.get(self.cur_pos())?;
		let item = Arc::clone(item);

		Some(item)
	}

	/// Returns the next image to load
	pub fn next(&self) -> Option<Arc<Path>> {
		let item = self.cur_items.get(self.next_pos())?;
		let item = Arc::clone(item);

		Some(item)
	}
}
