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

	/// Parallax
	pub parallax: PanelParallaxState,
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

/// Parallax state
#[derive(Debug)]
pub struct PanelParallaxState {
	/// Parallax scale, 0.0 .. 1.0
	// TODO: Rename to `scale`?
	pub ratio: f32,

	/// Parallax exponentiation
	pub exp: f32,

	/// Reverse parallax
	pub reverse: bool,
}
