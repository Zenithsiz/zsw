//! Fetch-update lock

/// Fetch-update value
///
/// Holds a value and tracks whether a value was fetched
/// yet since last updated
#[derive(Debug)]
pub struct FetchUpdate<T> {
	/// Value
	value: T,

	/// If this value has been seen
	seen: bool,
}

impl<T> FetchUpdate<T> {
	/// Creates a new fetch-update lock
	pub fn new(value: T) -> Self {
		Self { value, seen: false }
	}

	/// Fetches the value
	pub fn fetch(&mut self) -> &T
	where
		T: Send,
	{
		// Set that the value was seen
		self.seen = true;

		// Then return it
		&self.value
	}

	/// Returns if the value has been seen
	pub fn is_seen(&self) -> bool {
		self.seen
	}

	/// Attempts to update the value, returns `Err` if previous value wasn't seen yet
	pub fn update(&mut self, value: T) -> Result<(), T> {
		match self.seen {
			// If it was seen, update it and set it as unseen
			true => {
				self.value = value;
				self.seen = false;
				Ok(())
			},
			false => Err(value),
		}
	}
}
