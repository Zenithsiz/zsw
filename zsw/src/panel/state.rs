//! Panel state

/// Panel state
#[derive(Debug)]
pub struct PanelState {
	/// If paused
	pub paused: bool,

	/// Current progress (in frames)
	pub progress: u64,

	/// Duration (in frames)
	pub duration: u64,

	/// Fade point (in frames)
	pub fade_point: u64,
}

impl PanelState {
	/// Returns the normalized image progress
	#[must_use]
	pub fn progress_norm(&self) -> f32 {
		// Note: Image progress is linear throughout the full cycle
		self.progress as f32 / self.duration as f32
	}

	/// Returns the fade point normalized
	pub fn fade_point_norm(&self) -> f32 {
		// Note: Image progress is linear throughout the full cycle
		self.fade_point as f32 / self.duration as f32
	}
}
