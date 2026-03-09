//! Frame times

// Imports
use core::marker::PhantomData;

/// Frame times
#[derive(Debug)]
pub struct FrameTimes<T>(PhantomData<T>);

impl<T> FrameTimes<T> {
	pub fn add(&mut self, _frame_time: T) {}

	pub fn pause(&mut self, _pause: bool) {}

	#[must_use]
	pub fn is_paused(&self) -> bool {
		false
	}

	#[must_use]
	pub fn max_len(&self) -> usize {
		0
	}

	pub fn set_max_len(&mut self, _max_len: usize) {}

	pub fn iter(&self) -> impl Iterator<Item = &T> {
		[].into_iter()
	}

	#[must_use]
	pub fn len(&self) -> usize {
		0
	}

	#[must_use]
	pub fn is_empty(&self) -> bool {
		true
	}
}

impl<T> Default for FrameTimes<T> {
	fn default() -> Self {
		Self(PhantomData)
	}
}
