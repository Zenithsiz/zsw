//! Locks

// Imports
use std::ops::{Deref, DerefMut};

/// Lock with an associated guard
#[derive(Debug)]
pub struct Lock<'a, Guard, Source> {
	/// Guard
	guard: Guard,

	/// Source
	// Note: This is just to ensure caller only passes a
	//       lock that came from the same instance that locked it.
	source: &'a Source,
}

impl<'a, Guard, Source> Lock<'a, Guard, Source> {
	/// Creates a new lock
	pub fn new(guard: Guard, source: &'a Source) -> Self {
		Self { guard, source }
	}

	/// Asserts that the correct `wgpu` instance was passed
	fn assert_source(&self, source: &Source) {
		assert_eq!(
			self.source as *const _, source as *const _,
			"Lock had the wrong source then used"
		);
	}
}

impl<'a, Guard, Source> Lock<'a, Guard, Source>
where
	Guard: Deref,
{
	/// Returns the inner data
	pub fn get(&self, source: &Source) -> &Guard::Target {
		self.assert_source(source);
		&self.guard
	}
}

impl<'a, Guard, Source> Lock<'a, Guard, Source>
where
	Guard: DerefMut,
{
	/// Returns the inner data
	pub fn get_mut(&mut self, source: &Source) -> &mut Guard::Target {
		self.assert_source(source);
		&mut self.guard
	}
}
