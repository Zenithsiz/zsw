//! Panel state

/// Panel state
#[derive(Debug)]
pub struct PanelState {
	/// Current progress (in frames)
	pub cur_progress: u64,

	/// Duration (in frames)
	pub duration: u64,

	/// Fade point (in frames)
	pub fade_point: u64,

	// TODO: Organize parallax better (maybe in shader?)
	/// Parallax scale, 0.0 .. 1.0
	pub parallax_ratio: f32,

	/// Parallax exponentiation
	pub parallax_exp: f32,

	/// Reverse parallax
	pub reverse_parallax: bool,
}

impl PanelState {
	/// Returns the next progress when using only primary image
	pub fn next_progress_primary_only(&self) -> u64 {
		self.cur_progress.saturating_add(1).clamp(0, self.fade_point)
	}

	/// Returns the next progress when using both images
	pub fn next_progress_both(&self) -> u64 {
		self.cur_progress.saturating_add(1).clamp(0, self.duration)
	}

	/// Returns the progress if a swap occurred right now
	pub fn back_swapped_progress(&self) -> u64 {
		// Note: This is the progress of the back image at the duration
		//       See `back_progress` for more details
		self.cur_progress.saturating_sub(self.fade_point)
	}

	/// Returns the normalized front image progress
	#[must_use]
	pub fn front_progress_norm(&self) -> f32 {
		// Note: Front image progress is linear throughout the full cycle
		self.cur_progress as f32 / self.duration as f32
	}

	/// Returns the normalized back image progress
	#[must_use]
	pub fn back_progress_norm(&self) -> f32 {
		// Note: Back image progress is linear (with the same ratio as the front) *after* the fade point.
		//       This ensures that when it swaps to the front image, it'll be at the correct place
		match self.cur_progress {
			f if f >= self.fade_point => (self.cur_progress - self.fade_point) as f32 / self.duration as f32,
			_ => 0.0,
		}
	}

	/// Returns the alpha of the front image
	pub fn front_alpha(&self) -> f32 {
		match self.cur_progress {
			f if f >= self.fade_point =>
				1.0 - (self.cur_progress - self.fade_point) as f32 / (self.duration - self.fade_point) as f32,
			_ => 1.0,
		}
	}
}
