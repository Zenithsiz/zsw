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
	renderer::{PanelShader, PanelsGeometryUniforms, PanelsRenderer, PanelsRendererLayouts},
	state::PanelState,
};

// Imports
use {
	crate::playlist::PlaylistPlayer,
	chrono::TimeDelta,
	core::{borrow::Borrow, fmt, time::Duration},
	std::sync::Arc,
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

	/// State
	pub state: PanelState,
}

impl Panel {
	/// Creates a new panel
	pub fn new(name: PanelName, geometries: Vec<Rect<i32, u32>>, state: PanelState) -> Self {
		Self {
			name,
			geometries: geometries.into_iter().map(PanelGeometry::new).collect(),
			state,
		}
	}

	/// Returns the max duration for the current image
	fn max_duration(images: &PanelImages, state: &PanelState) -> Duration {
		match (images.cur.is_loaded(), images.next.is_loaded()) {
			(false, false) => Duration::ZERO,
			(true, false) => state.duration - state.fade_duration,
			(_, true) => state.duration,
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
			Ok(()) => self.state.progress = self.state.duration.saturating_sub(self.state.fade_duration),
			Err(()) => self.state.progress = Self::max_duration(images, &self.state),
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
			true => Some(self.state.progress.saturating_add(delta_abs)),
			false => self.state.progress.checked_sub(delta_abs),
		};

		// Update the progress, potentially rolling over to the previous/next image
		self.state.progress = match next_progress {
			Some(next_progress) => match next_progress >= self.state.duration {
				true => match images.step_next(wgpu_shared, renderer_layouts) {
					Ok(()) => next_progress - self.state.duration,
					Err(()) => Self::max_duration(images, &self.state),
				},
				false => next_progress.clamp(Duration::ZERO, Self::max_duration(images, &self.state)),
			},
			None => match images.step_prev(wgpu_shared, renderer_layouts) {
				// Note: This branch is only taken when `delta` is negative, so we can always
				//       subtract without checking `delta_is_positive`.
				Ok(()) => match (self.state.duration + self.state.progress).checked_sub(delta_abs) {
					Some(next_progress) => next_progress,
					None => self.state.fade_duration,
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
		if self.state.paused {
			return;
		}

		self.step(images, wgpu_shared, renderer_layouts, delta);
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
