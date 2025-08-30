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

		match self
			.state
			.images
			.step_next(playlist_player, wgpu_shared, renderer_layouts)
		{
			Ok(()) => self.state.progress = self.state.duration.saturating_sub(self.state.fade_duration),
			Err(()) => self.state.progress = self.state.max_duration(),
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
			Some(next_progress) => match next_progress >= self.state.duration {
				true => match self
					.state
					.images
					.step_next(playlist_player, wgpu_shared, renderer_layouts)
				{
					Ok(()) => next_progress - self.state.duration,
					Err(()) => self.state.max_duration(),
				},
				false => next_progress.clamp(Duration::ZERO, self.state.max_duration()),
			},
			None => match self
				.state
				.images
				.step_prev(playlist_player, wgpu_shared, renderer_layouts)
			{
				// Note: This branch is only taken when `delta` is negative, so we can always
				//       subtract without checking `delta_is_positive`.
				Ok(()) => match (self.state.duration + self.state.progress).checked_sub(delta_abs) {
					Some(next_progress) => next_progress,
					None => self.state.fade_duration,
				},

				Err(()) => Duration::ZERO,
			},
		};
	}

	/// Updates this panel's state using the current time as a delta
	///
	/// If the playlist player isn't loaded, does nothing
	pub fn update(&mut self, wgpu_shared: &WgpuShared, renderer_layouts: &PanelsRendererLayouts) {
		let Some(playlist_player) = &mut self.playlist_player else {
			return;
		};

		// Calculate the delta since the last update and update it
		// TODO: If the delta is small enough (<1ms), skip updating?
		//       This happens when we have multiple renderers rendering
		//       at the same time, one to try to update immediately after
		//       the other has updated.
		// TODO: This can fall out of sync after a lot of cycles due to precision,
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

		let (delta_abs, delta_is_positive) = self::time_delta_to_duration(delta);
		let next_progress = match delta_is_positive {
			true => Some(self.state.progress.saturating_add(delta_abs)),
			false => self.state.progress.checked_sub(delta_abs),
		};

		// Update the progress, potentially rolling over to the previous/next image
		self.state.progress = match next_progress {
			Some(next_progress) => match next_progress >= self.state.duration {
				true => match self
					.state
					.images
					.step_next(playlist_player, wgpu_shared, renderer_layouts)
				{
					Ok(()) => next_progress - self.state.duration,
					Err(()) => self.state.max_duration(),
				},
				false => next_progress.clamp(Duration::ZERO, self.state.max_duration()),
			},
			None => match self
				.state
				.images
				.step_prev(playlist_player, wgpu_shared, renderer_layouts)
			{
				// Note: This branch is only taken when `delta` is negative, so we can always
				//       subtract without checking `delta_is_positive`.
				Ok(()) => match (self.state.duration + self.state.progress).checked_sub(delta_abs) {
					Some(next_progress) => next_progress,
					None => self.state.fade_duration,
				},

				Err(()) => Duration::ZERO,
			},
		};
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
