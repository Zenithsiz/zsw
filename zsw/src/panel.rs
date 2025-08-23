//! Panel

// Modules
mod geometry;
mod images;
mod loader;
mod renderer;
mod ser;
mod state;

// Exports
pub use self::{
	geometry::PanelGeometry,
	images::{PanelImage, PanelImages},
	loader::PanelsLoader,
	renderer::{PanelShader, PanelsGeometryUniforms, PanelsRenderer, PanelsRendererLayouts},
	state::PanelState,
};

// Imports
use {
	crate::{image_loader::ImageRequester, playlist::PlaylistPlayer},
	core::{borrow::Borrow, fmt},
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

	/// Shader
	pub shader: PanelShader,
}

impl Panel {
	/// Creates a new panel
	pub fn new(name: PanelName, geometries: Vec<Rect<i32, u32>>, state: PanelState, shader: PanelShader) -> Self {
		Self {
			name,
			geometries: geometries.into_iter().map(PanelGeometry::new).collect(),
			state,
			shader,
		}
	}

	/// Returns the max duration for the current image
	fn max_duration(images: &PanelImages, state: &PanelState) -> u64 {
		match (images.cur().is_loaded(), images.next().is_loaded()) {
			(false, false) => 0,
			(true, false) => state.fade_point,
			(_, true) => state.duration,
		}
	}

	/// Skips to the next image.
	///
	/// If the images aren't loaded, does nothing
	pub async fn skip(
		&mut self,
		images: &mut PanelImages,
		wgpu_shared: &WgpuShared,
		renderer_layouts: &PanelsRendererLayouts,
		image_requester: &ImageRequester,
	) {
		match images.step_next(wgpu_shared, renderer_layouts).await {
			Ok(()) => self.state.progress = self.state.duration.saturating_sub(self.state.fade_point),
			Err(()) => self.state.progress = Self::max_duration(images, &self.state),
		}

		// Then load any missing images
		images
			.load_missing(wgpu_shared, renderer_layouts, image_requester, &self.geometries)
			.await;
	}

	/// Steps this panel's state by a certain number of frames (potentially negative).
	///
	/// If the images aren't loaded, does nothing
	pub async fn step(
		&mut self,
		images: &mut PanelImages,
		wgpu_shared: &WgpuShared,
		renderer_layouts: &PanelsRendererLayouts,
		image_requester: &ImageRequester,
		frames: i64,
	) {
		// Update the progress, potentially rolling over to the previous/next image
		self.state.progress = match self.state.progress.checked_add_signed(frames) {
			Some(next_progress) => match next_progress >= self.state.duration {
				true => match images.step_next(wgpu_shared, renderer_layouts).await {
					Ok(()) => next_progress.saturating_sub(self.state.duration),
					Err(()) => Self::max_duration(images, &self.state),
				},
				false => self
					.state
					.progress
					.saturating_add_signed(frames)
					.clamp(0, Self::max_duration(images, &self.state)),
			},
			None => match images.step_prev(wgpu_shared, renderer_layouts).await {
				Ok(()) => self.state.duration.saturating_add_signed(frames),
				Err(()) => 0,
			},
		};

		// Then load any missing images
		images
			.load_missing(wgpu_shared, renderer_layouts, image_requester, &self.geometries)
			.await;
	}

	/// Updates this panel's state
	///
	/// If the images aren't loaded, does nothing
	pub async fn update(
		&mut self,
		images: &mut PanelImages,
		wgpu_shared: &WgpuShared,
		renderer_layouts: &PanelsRendererLayouts,
		image_requester: &ImageRequester,
	) {
		// Then load any missing images
		images
			.load_missing(wgpu_shared, renderer_layouts, image_requester, &self.geometries)
			.await;

		// If we're paused, don't update anything
		if self.state.paused {
			return;
		}

		self.step(images, wgpu_shared, renderer_layouts, image_requester, 1)
			.await;
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
