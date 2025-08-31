//! Panel state

// Imports
use {
	super::{PanelImages, PanelShader, PanelsRendererLayouts},
	crate::playlist::PlaylistPlayer,
	chrono::TimeDelta,
	core::time::Duration,
	std::time::Instant,
	zsw_wgpu::WgpuShared,
};

/// Panel state
#[derive(Debug)]
pub struct PanelState {
	/// If paused
	paused: bool,

	/// Shader
	shader: PanelShader,

	/// Last update
	last_update: Instant,

	/// Current progress
	progress: Duration,

	/// Duration
	duration: Duration,

	/// Fade duration
	fade_duration: Duration,

	/// Images, if loaded
	images: PanelImages,
}

impl PanelState {
	pub fn new(duration: Duration, fade_duration: Duration, shader: PanelShader, images: PanelImages) -> Self {
		Self {
			paused: false,
			shader,
			last_update: Instant::now(),
			progress: Duration::ZERO,
			duration,
			fade_duration,
			images,
		}
	}

	/// Returns the image progress
	pub fn progress(&self) -> Duration {
		self.progress
	}

	/// Sets the image progress
	pub fn set_progress(&mut self, progress: Duration) {
		self.progress = progress.clamp(self.min_progress(), self.max_progress());
	}

	/// Returns the normalized image progress
	#[must_use]
	pub fn progress_norm(&self) -> f32 {
		// Note: Image progress is linear throughout the full cycle
		self.progress.div_duration_f32(self.duration)
	}

	/// Returns the image fade duration
	pub fn fade_duration(&self) -> Duration {
		self.fade_duration
	}

	/// Sets the fade duration
	pub fn set_fade_duration(&mut self, fade_duration: Duration) {
		self.fade_duration = fade_duration.min(self.duration() / 2);
		self.set_progress(self.progress);
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

	/// Returns the image duration
	pub fn duration(&self) -> Duration {
		self.duration
	}

	/// Sets the duration
	pub fn set_duration(&mut self, duration: Duration) {
		self.duration = duration;
		self.set_fade_duration(self.fade_duration);
	}

	/// Returns the panel shader
	pub fn shader(&self) -> PanelShader {
		self.shader
	}

	/// Sets the panel shader
	pub fn set_shader(&mut self, shader: PanelShader) {
		self.shader = shader;
	}

	/// Returns the panel images
	pub fn images(&self) -> &PanelImages {
		&self.images
	}

	/// Returns the panel images mutably
	pub fn images_mut(&mut self) -> &mut PanelImages {
		&mut self.images
	}

	/// Returns if paused
	pub fn is_paused(&self) -> bool {
		self.paused
	}

	/// Update the last time this field was updated and returns
	/// the duration since that update
	fn update_delta(&mut self) -> Duration {
		// TODO: this can fall out of sync after a lot of cycles due to precision,
		//       should we do it in some other way?
		let now = Instant::now();
		let delta = now.duration_since(self.last_update);
		self.last_update = now;

		delta
	}

	/// Sets the pause state
	pub fn set_paused(&mut self, paused: bool) {
		self.paused = paused;

		// Note: If we're unpausing, we don't want to skip ahead
		//       due to the last update being in the past, so just
		//       set it to now
		if !self.paused {
			self.last_update = Instant::now();
		}
	}

	/// Toggles pause of this state
	pub fn toggle_paused(&mut self) {
		self.set_paused(!self.paused);
	}

	/// Skips to the next image.
	///
	/// If the playlist player isn't loaded, does nothing
	pub fn skip(
		&mut self,
		playlist_player: &mut PlaylistPlayer,
		wgpu_shared: &WgpuShared,
		renderer_layouts: &PanelsRendererLayouts,
	) {
		self.progress = match self.images.step_next(playlist_player, wgpu_shared, renderer_layouts) {
			Ok(()) => Duration::ZERO,
			Err(()) => self.max_progress(),
		}
	}

	/// Steps this panel's state by a certain number of frames (potentially negative).
	///
	/// If the playlist player isn't loaded, does nothing
	pub fn step(
		&mut self,
		playlist_player: &mut PlaylistPlayer,
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
			// If we have a next progress, check if we overflowed the duration
			Some(next_progress) => match next_progress.checked_sub(self.duration) {
				// If we did, `next_progress` is our progress at the next image, so try
				// to step to it.
				Some(next_progress) => match self.images.step_next(playlist_player, wgpu_shared, renderer_layouts) {
					// If we successfully stepped to the next image, start at the next progress
					// Note: If delta was big enough to overflow 2 durations, then cap it at the
					//       max duration of the next image.
					Ok(()) => next_progress.min(self.max_progress()),

					// Otherwise, stay at most on our max duration
					Err(()) => self.max_progress(),
				},

				// Otherwise, we're just moving within the current image, so clamp it
				// between our min and max progress
				None => next_progress.clamp(self.min_progress(), self.max_progress()),
			},

			// Otherwise, we underflowed, so try to step back
			None => match self.images.step_prev(playlist_player, wgpu_shared, renderer_layouts) {
				// If we successfully stepped backwards, start at where we're supposed to:
				Ok(()) => {
					// Note: This branch is only taken when `delta` is negative, so we can always
					//       subtract without checking `delta_is_positive`.
					assert!(!delta_is_positive, "Delta was negative despite having no next duration");

					// Note: If this delta actually underflowed twice, cap it at the minimum
					//       progress of the previous image instead.
					match (self.duration + self.progress).checked_sub(delta_abs) {
						Some(next_progress) => next_progress,
						None => self.min_progress(),
					}
				},

				// Otherwise, just stay at the minimum progress of the current image.
				Err(()) => self.min_progress(),
			},
		}
	}

	/// Updates this panel's state using the current time as a delta
	///
	/// If the playlist player isn't loaded, does nothing
	pub fn update(
		&mut self,
		playlist_player: &mut PlaylistPlayer,
		wgpu_shared: &WgpuShared,
		renderer_layouts: &PanelsRendererLayouts,
	) {
		// Note: We always load images, even if we're paused, since the user might be
		//       moving around manually.
		self.images.load_missing(playlist_player, wgpu_shared, renderer_layouts);

		// If we're paused, don't update anything
		if self.paused {
			return;
		}

		// Calculate the delta since the last update and step through it
		// TODO: If the delta is small enough (<1ms), skip updating?
		//       this happens when we have multiple renderers rendering
		//       at the same time, one to try to update immediately after
		//       the other has updated.
		let delta = self.update_delta();
		let delta = TimeDelta::from_std(delta).expect("Last update duration didn't fit into a delta");
		self.step(playlist_player, wgpu_shared, renderer_layouts, delta);
	}
}

/// Converts a chrono time delta into a duration, indicating whether it's positive or negative
fn time_delta_to_duration(delta: TimeDelta) -> (Duration, bool) {
	match delta.to_std() {
		Ok(delta) => (delta, true),
		Err(_) => ((-delta).to_std().expect("Duration should fit"), false),
	}
}
