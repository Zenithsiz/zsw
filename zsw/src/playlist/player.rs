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
	items: HashSet<Arc<Path>>,

	/// Previous items
	///
	/// Last item is newest item
	prev_items: VecDeque<Arc<Path>>,

	/// Maximum number of previous items
	max_prev_items: usize,

	/// Next items
	///
	/// Last item is next item
	next_items: Vec<Arc<Path>>,

	/// Rng
	rng: StdRng,
}

impl PlaylistPlayer {
	/// Creates a new, empty, player
	pub fn new() -> Self {
		Self {
			items:          HashSet::new(),
			prev_items:     VecDeque::new(),
			max_prev_items: 64,
			next_items:     vec![],
			rng:            StdRng::from_entropy(),
		}
	}

	/// Adds an item to the playlist
	pub fn add(&mut self, path: Arc<Path>) {
		// TODO: Should we care if the item was already in?
		let _ = self.items.insert(path);
	}

	/// Removes an item from the playlist
	pub fn remove(&mut self, path: &Path) {
		// Remove the item from all our playlists
		// TODO: Not have `O(N)` complexity on prev / next items
		let _ = self.items.remove(path);
		self.prev_items.retain(|item| &**item != path);
		self.next_items.retain(|item| &**item != path);
	}

	/// Removes all paths from the playlist
	pub fn remove_all(&mut self) {
		self.items.clear();
		self.prev_items.clear();
		self.next_items.clear();
	}

	/// Clears the current backlog
	// TODO: Better wording than backlog: deck, remaining items?
	pub fn clear_backlog(&mut self) {
		self.next_items.clear();
	}

	/// Returns an iterator over all items in the playlist
	pub fn all_items(&self) -> impl ExactSizeIterator<Item = &Arc<Path>> {
		self.items.iter()
	}

	/// Returns an iterator over all consumed items
	///
	/// They are ordered from newest to oldest
	pub fn prev_items(&self) -> impl ExactSizeIterator<Item = &Arc<Path>> {
		self.prev_items.iter().rev()
	}

	/// Returns an iterator that peeks over the remaining items in this loop.
	///
	/// They are ordered from next to last
	pub fn peek_next_items(&self) -> impl ExactSizeIterator<Item = &Arc<Path>> {
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
		self.prev_items.push_back(Arc::clone(&item));
		if self.prev_items.len() > self.max_prev_items {
			let _ = self.prev_items.pop_front();
		}

		Some(item)
	}
}
