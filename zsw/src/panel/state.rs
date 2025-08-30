//! Panel state

// Imports
use {
	super::{PanelImages, PanelShader},
	core::time::Duration,
	std::time::Instant,
};

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

	/// Images, if loaded
	pub images: PanelImages,
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

	/// Returns the min progress for the current image
	pub fn min_progress(&self) -> Duration {
		match self.images.prev.is_loaded() {
			true => Duration::ZERO,
			false => self.fade_duration,
		}
	}

	/// Returns the max progress for the current image
	pub fn max_progress(&self) -> Duration {
		match self.images.next.is_loaded() {
			true => self.duration,
			false => self.duration - self.fade_duration,
		}
	}

	/// Toggles pause of this state
	pub fn toggle_pause(&mut self) {
		self.paused ^= true;

		// Note: If we're unpausing, we don't want to skip ahead
		//       due to the last update being in the past, so just
		//       set it to now
		if !self.paused {
			self.last_update = Instant::now();
		}
	}
}
