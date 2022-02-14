//! Locks

// Imports
use parking_lot::MutexGuard;

/// Lock with an associated guard
// TODO: Use `lock_api` to make this cleaner
#[derive(Debug)]
pub struct Lock<'a, Data, Source> {
	/// Guard
	guard: MutexGuard<'a, Data>,

	/// Source pointer
	// Note: This is just to ensure caller only passes a
	//       lock that came from the same instance that locked it.
	source: *const Source,
}

impl<'a, Data, Source> Lock<'a, Data, Source> {
	/// Creates a new lock
	pub fn new(guard: MutexGuard<'a, Data>, source: &Source) -> Self {
		Self { guard, source }
	}

	/// Returns the inner data
	pub fn get(&self, source: &Source) -> &Data {
		self.assert_source(source);
		&self.guard
	}

	/// Returns the inner data
	pub fn get_mut(&mut self, source: &Source) -> &mut Data {
		self.assert_source(source);
		&mut self.guard
	}

	/// Asserts that the correct `wgpu` instance was passed
	fn assert_source(&self, source: &Source) {
		assert_eq!(self.source, source as *const _, "Lock had the wrong source then used");
	}
}
