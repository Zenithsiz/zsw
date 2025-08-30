//! Panel state

// Imports
use {
	super::{PanelImages, PanelShader, PanelsRendererLayouts},
	chrono::TimeDelta,
	core::time::Duration,
	std::time::Instant,
	zsw_wgpu::WgpuShared,
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

	/// Returns the max duration for the current image
	fn max_duration(&self, images: &PanelImages) -> Duration {
		match (images.cur.is_loaded(), images.next.is_loaded()) {
			(false, false) => Duration::ZERO,
			(true, false) => self.duration - self.fade_duration,
			(_, true) => self.duration,
		}
	}

	/// Skips to the next image.
	///
	/// If the images aren't loaded, does nothing
	pub fn skip(
		&mut self,
		images: &mut PanelImages,
		wgpu_shared: &WgpuShared,
		renderer_layouts: &PanelsRendererLayouts,
	) {
		match images.step_next(wgpu_shared, renderer_layouts) {
			Ok(()) => self.progress = self.duration.saturating_sub(self.fade_duration),
			Err(()) => self.progress = self.max_duration(images),
		}

		// Then load any missing images
		images.load_missing(wgpu_shared, renderer_layouts);
	}

	/// Steps this panel's state by a certain number of frames (potentially negative).
	///
	/// If the images aren't loaded, does nothing
	pub fn step(
		&mut self,
		images: &mut PanelImages,
		wgpu_shared: &WgpuShared,
		renderer_layouts: &PanelsRendererLayouts,
		delta: TimeDelta,
	) {
		let (delta_abs, delta_is_positive) = self::time_delta_to_duration(delta);
		let next_progress = match delta_is_positive {
			true => Some(self.progress.saturating_add(delta_abs)),
			false => self.progress.checked_sub(delta_abs),
		};

		// Update the progress, potentially rolling over to the previous/next image
		self.progress = match next_progress {
			Some(next_progress) => match next_progress >= self.duration {
				true => match images.step_next(wgpu_shared, renderer_layouts) {
					Ok(()) => next_progress - self.duration,
					Err(()) => self.max_duration(images),
				},
				false => next_progress.clamp(Duration::ZERO, self.max_duration(images)),
			},
			None => match images.step_prev(wgpu_shared, renderer_layouts) {
				// Note: This branch is only taken when `delta` is negative, so we can always
				//       subtract without checking `delta_is_positive`.
				Ok(()) => match (self.duration + self.progress).checked_sub(delta_abs) {
					Some(next_progress) => next_progress,
					None => self.fade_duration,
				},

				Err(()) => Duration::ZERO,
			},
		};

		// Then load any missing images
		images.load_missing(wgpu_shared, renderer_layouts);
	}

	/// Updates this panel's state
	///
	/// If the images aren't loaded, does nothing
	pub fn update(
		&mut self,
		images: &mut PanelImages,
		wgpu_shared: &WgpuShared,
		renderer_layouts: &PanelsRendererLayouts,

		delta: TimeDelta,
	) {
		// Then load any missing images
		images.load_missing(wgpu_shared, renderer_layouts);

		// If we're paused, don't update anything
		if self.paused {
			return;
		}

		self.step(images, wgpu_shared, renderer_layouts, delta);
	}
}

/// Converts a chrono time delta into a duration, indicating whether it's positive or negative
fn time_delta_to_duration(delta: TimeDelta) -> (Duration, bool) {
	match delta.to_std() {
		Ok(delta) => (delta, true),
		Err(_) => ((-delta).to_std().expect("Duration should fit"), false),
	}
}
