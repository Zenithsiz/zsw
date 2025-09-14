//! Panel state

// Imports
use {
	super::{PanelFadeImages, PanelFadeShader, PanelShader, PanelSlideShader},
	crate::playlist::PlaylistPlayer,
	chrono::TimeDelta,
	core::time::Duration,
	std::{sync::Arc, time::Instant},
	tokio::sync::Mutex,
	zsw_wgpu::Wgpu,
};

/// Panel state
#[derive(Debug)]
#[expect(clippy::large_enum_variant, reason = "Indirections are more costly")]
pub enum PanelState {
	/// None shader
	None(PanelNoneState),

	/// Fade shader
	Fade(PanelFadeState),

	/// Slide shader
	Slide(PanelSlideState),
}

impl PanelState {
	/// Returns the shader of this state
	pub fn shader(&self) -> PanelShader {
		match self {
			Self::None(state) => PanelShader::None {
				background_color: state.background_color,
			},
			Self::Fade(state) => PanelShader::Fade(state.shader()),
			Self::Slide(state) => PanelShader::Slide(state.shader()),
		}
	}
}

/// Panel none state
#[derive(Debug)]
pub struct PanelNoneState {
	/// Background color
	pub background_color: [f32; 4],
}

impl PanelNoneState {
	/// Creates new state
	pub fn new(background_color: [f32; 4]) -> Self {
		Self { background_color }
	}
}

/// Panel fade state
#[derive(Debug)]
pub struct PanelFadeState {
	/// If paused
	paused: bool,

	/// Shader
	shader: PanelFadeShader,

	/// Last update
	last_update: Instant,

	/// Current progress
	progress: Duration,

	/// Duration
	duration: Duration,

	/// Fade duration
	fade_duration: Duration,

	/// Images
	images: PanelFadeImages,

	/// Playlist player
	playlist_player: Arc<Mutex<PlaylistPlayer>>,
}

impl PanelFadeState {
	pub fn new(duration: Duration, fade_duration: Duration, shader: PanelFadeShader) -> Self {
		let playlist_player = Arc::new(Mutex::new(PlaylistPlayer::new()));

		Self {
			paused: false,
			shader,
			last_update: Instant::now(),
			progress: Duration::ZERO,
			duration,
			fade_duration,
			images: PanelFadeImages::new(),
			playlist_player,
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
		match self.images.prev.is_some() {
			// If we have a previous image, we can go until the very beginning
			true => Duration::ZERO,

			// Otherwise, stop before the fade
			false => self.fade_duration,
		}
	}

	/// Returns the max progress for the current image
	pub fn max_progress(&self) -> Duration {
		match (self.images.cur.is_some(), self.images.next.is_some()) {
			// If we have a next image, we can go until the full duration
			(_, true) => self.duration,

			// Otherwise, if we have a current, but no next, we can go until the fade begins
			(true, false) => self.duration - self.fade_duration,

			// Finally, if we don't have any, we should stay at the beginning
			(false, false) => self.min_progress(),
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
	pub fn shader(&self) -> PanelFadeShader {
		self.shader
	}

	/// Returns the panel shader mutably
	pub fn shader_mut(&mut self) -> &mut PanelFadeShader {
		&mut self.shader
	}

	/// Returns the panel images
	pub fn images(&self) -> &PanelFadeImages {
		&self.images
	}

	/// Returns the panel images mutably
	pub fn images_mut(&mut self) -> &mut PanelFadeImages {
		&mut self.images
	}

	/// Returns the panel playlist player
	pub fn playlist_player(&self) -> &Arc<Mutex<PlaylistPlayer>> {
		&self.playlist_player
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
	pub async fn skip(&mut self, wgpu: &Wgpu) {
		let mut playlist_player = self.playlist_player.lock().await;
		self.progress = match self.images.step_next(&mut playlist_player, wgpu) {
			Ok(()) => self.fade_duration,
			Err(()) => self.max_progress(),
		}
	}

	/// Steps this panel's state by a certain number of frames (potentially negative).
	pub async fn step(&mut self, wgpu: &Wgpu, delta: TimeDelta) {
		let mut playlist_player = self.playlist_player.lock().await;

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
				Some(next_progress) => match self.images.step_next(&mut playlist_player, wgpu) {
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
			None => match self.images.step_prev(&mut playlist_player, wgpu) {
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
	pub async fn update(&mut self, wgpu: &Wgpu) {
		// Note: We always load images, even if we're paused, since the user might be
		//       moving around manually.
		self.images.load_missing(&mut *self.playlist_player.lock().await, wgpu);

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
		self.step(wgpu, delta).await;
	}
}

/// Panel slide state
#[derive(Debug)]
pub struct PanelSlideState {
	/// Shader
	shader: PanelSlideShader,
}

impl PanelSlideState {
	/// Creates new state
	pub fn new(shader: PanelSlideShader) -> Self {
		Self { shader }
	}

	/// Returns the panel shader
	pub fn shader(&self) -> PanelSlideShader {
		self.shader
	}

	/// Returns the panel shader mutably
	pub fn shader_mut(&mut self) -> &mut PanelSlideShader {
		&mut self.shader
	}
}

/// Converts a chrono time delta into a duration, indicating whether it's positive or negative
fn time_delta_to_duration(delta: TimeDelta) -> (Duration, bool) {
	match delta.to_std() {
		Ok(delta) => (delta, true),
		Err(_) => ((-delta).to_std().expect("Duration should fit"), false),
	}
}
