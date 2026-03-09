//! Frame times

// Imports
use std::collections::VecDeque;

/// Frame times
#[derive(Debug)]
pub struct FrameTimes<T> {
	/// Times
	times: VecDeque<T>,

	/// Maximum frame times
	max_len: usize,

	/// Paused
	paused: bool,
}

impl<T> FrameTimes<T> {
	/// Adds a frame time to these metrics
	pub fn add(&mut self, frame_time: T) {
		if self.paused {
			return;
		}

		self.times.push_back(frame_time);
		if self.times.len() > self.max_len {
			_ = self.times.drain(..self.times.len() - self.max_len);
		}
	}

	/// Pauses these metrics
	pub fn pause(&mut self, pause: bool) {
		self.paused = pause;
	}

	/// Returns if these metrics are paused
	#[must_use]
	pub fn is_paused(&self) -> bool {
		self.paused
	}

	/// Returns the max number of frame times kept in these metrics
	#[must_use]
	pub fn max_len(&self) -> usize {
		self.max_len
	}

	/// Sets the max number of frame times kept in these metrics
	pub fn set_max_len(&mut self, max_len: usize) {
		self.max_len = max_len;
	}

	/// Returns an iterator over the frame times in these metrics
	pub fn iter(&self) -> impl Iterator<Item = &T> {
		self.times.iter()
	}

	/// Returns the number of frame times in these metrics
	#[must_use]
	pub fn len(&self) -> usize {
		self.times.len()
	}

	/// Returns if these metrics are empty
	#[must_use]
	pub fn is_empty(&self) -> bool {
		self.times.is_empty()
	}
}

impl<T> Default for FrameTimes<T> {
	fn default() -> Self {
		Self {
			times:   VecDeque::new(),
			// 10 seconds worth on 60 Hz
			max_len: 60 * 10,
			paused:  false,
		}
	}
}
