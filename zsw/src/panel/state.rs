//! Panel state

// Imports
use {super::PanelShader, core::time::Duration, std::time::Instant};

/// Panel state
#[derive(Debug)]
pub struct PanelState {
	/// If paused
	pub paused: bool,

	/// Shader
	pub shader: PanelShader,

	/// Last update
	pub last_update: Instant,

	/// Current progress
	pub progress: Duration,

	/// Duration
	pub duration: Duration,

	/// Fade duration
	pub fade_duration: Duration,
}

impl PanelState {
	/// Returns the normalized image progress
	#[must_use]
	pub fn progress_norm(&self) -> f32 {
		// Note: Image progress is linear throughout the full cycle
		self.progress.div_duration_f32(self.duration)
	}

	/// Returns the fade duration normalized
	pub fn fade_duration_norm(&self) -> f32 {
		// Note: Image progress is linear throughout the full cycle
		self.fade_duration.div_duration_f32(self.duration)
	}
}
