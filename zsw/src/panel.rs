//! Panel

// Modules
mod geometry;
mod images;
mod panels;
mod renderer;
mod ser;
mod state;

// Exports
pub use self::{
	geometry::PanelGeometry,
	images::{PanelImage, PanelImages},
	panels::Panels,
	renderer::{PanelShader, PanelShaderFade, PanelsGeometryUniforms, PanelsRenderer, PanelsRendererLayouts},
	state::PanelState,
};

// Imports
use {
	crate::playlist::PlaylistPlayer,
	chrono::TimeDelta,
	core::{borrow::Borrow, fmt, time::Duration},
	std::{sync::Arc, time::Instant},
	zsw_util::Rect,
	zsw_wgpu::WgpuShared,
};

/// Panel
#[derive(Debug)]
pub struct Panel {
	/// Name
	pub name: PanelName,

	/// Geometries
	pub geometries: Vec<PanelGeometry>,

	/// Playlist player
	pub playlist_player: Option<PlaylistPlayer>,

	/// State
	pub state: PanelState,
}

impl Panel {
	/// Creates a new panel
	pub fn new(name: PanelName, geometries: Vec<Rect<i32, u32>>, state: PanelState) -> Self {
		Self {
			name,
			geometries: geometries.into_iter().map(PanelGeometry::new).collect(),
			playlist_player: None,
			state,
		}
	}

	/// Skips to the next image.
	///
	/// If the playlist player isn't loaded, does nothing
	pub fn skip(&mut self, wgpu_shared: &WgpuShared, renderer_layouts: &PanelsRendererLayouts) {
		let Some(playlist_player) = &mut self.playlist_player else {
			return;
		};

		self.state.progress = match self
			.state
			.images
			.step_next(playlist_player, wgpu_shared, renderer_layouts)
		{
			Ok(()) => Duration::ZERO,
			Err(()) => self.state.max_progress(),
		}
	}

	/// Steps this panel's state by a certain number of frames (potentially negative).
	///
	/// If the playlist player isn't loaded, does nothing
	pub fn step(&mut self, wgpu_shared: &WgpuShared, renderer_layouts: &PanelsRendererLayouts, delta: TimeDelta) {
		let Some(playlist_player) = &mut self.playlist_player else {
			return;
		};

		let (delta_abs, delta_is_positive) = self::time_delta_to_duration(delta);
		let next_progress = match delta_is_positive {
			true => Some(self.state.progress.saturating_add(delta_abs)),
			false => self.state.progress.checked_sub(delta_abs),
		};

		// Update the progress, potentially rolling over to the previous/next image
		self.state.progress = match next_progress {
			// If we have a next progress, check if we overflowed the duration
			Some(next_progress) => match next_progress.checked_sub(self.state.duration) {
				// If we did, `next_progress` is our progress at the next image, so try
				// to step to it.
				Some(next_progress) => match self
					.state
					.images
					.step_next(playlist_player, wgpu_shared, renderer_layouts)
				{
					// If we successfully stepped to the next image, start at the next progress
					// Note: If delta was big enough to overflow 2 durations, then cap it at the
					//       max duration of the next image.
					Ok(()) => next_progress.min(self.state.max_progress()),

					// Otherwise, stay at most on our max duration
					Err(()) => self.state.max_progress(),
				},

				// Otherwise, we're just moving within the current image, so clamp it
				// between our min and max progress
				None => next_progress.clamp(self.state.min_progress(), self.state.max_progress()),
			},

			// Otherwise, we underflowed, so try to step back
			None => match self
				.state
				.images
				.step_prev(playlist_player, wgpu_shared, renderer_layouts)
			{
				// If we successfully stepped backwards, start at where we're supposed to:
				Ok(()) => {
					// Note: This branch is only taken when `delta` is negative, so we can always
					//       subtract without checking `delta_is_positive`.
					assert!(!delta_is_positive, "Delta was negative despite having no next duration");

					// Note: If this delta actually underflowed twice, cap it at the minimum
					//       progress of the previous image instead.
					match (self.state.duration + self.state.progress).checked_sub(delta_abs) {
						Some(next_progress) => next_progress,
						None => self.state.min_progress(),
					}
				},

				// Otherwise, just stay at the minimum progress of the current image.
				Err(()) => self.state.min_progress(),
			},
		}
	}

	/// Updates this panel's state using the current time as a delta
	///
	/// If the playlist player isn't loaded, does nothing
	pub fn update(&mut self, wgpu_shared: &WgpuShared, renderer_layouts: &PanelsRendererLayouts) {
		let Some(playlist_player) = &mut self.playlist_player else {
			return;
		};

		// Calculate the delta since the last update and update it
		// Note: Even if paused we still do this, to avoid the user unpausing
		//       and suddenly jumping forward
		// TODO: If the delta is small enough (<1ms), skip updating?
		//       this happens when we have multiple renderers rendering
		//       at the same time, one to try to update immediately after
		//       the other has updated.
		// TODO: this can fall out of sync after a lot of cycles due to precision,
		//       should we do it in some other way?
		let now = Instant::now();
		let delta = now.duration_since(self.state.last_update);
		self.state.last_update = now;
		let delta = TimeDelta::from_std(delta).expect("Frame duration did not fit into time delta");


		// Note: We always load images, even if we're paused, since the user might be
		//       moving around manually.
		self.state
			.images
			.load_missing(playlist_player, wgpu_shared, renderer_layouts);

		// If we're paused, don't update anything
		if self.state.paused {
			return;
		}

		self.step(wgpu_shared, renderer_layouts, delta);
	}
}

/// Panel name
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub struct PanelName(Arc<str>);

impl From<String> for PanelName {
	fn from(s: String) -> Self {
		Self(s.into())
	}
}

impl Borrow<str> for PanelName {
	fn borrow(&self) -> &str {
		&self.0
	}
}

impl fmt::Display for PanelName {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		self.0.fmt(f)
	}
}

impl fmt::Debug for PanelName {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		self.0.fmt(f)
	}
}

/// Converts a chrono time delta into a duration, indicating whether it's positive or negative
fn time_delta_to_duration(delta: TimeDelta) -> (Duration, bool) {
	match delta.to_std() {
		Ok(delta) => (delta, true),
		Err(_) => ((-delta).to_std().expect("Duration should fit"), false),
	}
}
