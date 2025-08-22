//! Panel

// Modules
mod geometry;
mod images;
mod renderer;
mod ser;
mod state;

// Exports
pub use self::{
	geometry::PanelGeometry,
	images::{PanelImage, PanelImages},
	renderer::{PanelShader, PanelsGeometryUniforms, PanelsRenderer, PanelsRendererLayouts},
	state::PanelState,
};

// Imports
use {
	crate::{AppError, image_loader::ImageRequester, playlist::PlaylistPlayer, shared::Shared},
	core::{borrow::Borrow, fmt},
	std::{path::PathBuf, sync::Arc},
	zsw_util::{PathAppendExt, Rect},
	zsw_wgpu::WgpuShared,
	zutil_app_error::Context,
};

/// Panels loader
#[derive(Debug)]
pub struct PanelsLoader {
	/// Panels directory
	root: PathBuf,
}

impl PanelsLoader {
	/// Creates a new panels loader
	pub fn new(root: PathBuf) -> Self {
		Self { root }
	}

	/// Loads a panel from a name.
	///
	/// If the panel isn't for this window, returns `Ok(None)`
	pub async fn load(
		&self,
		panel_name: PanelName,
		playlist_player: PlaylistPlayer,
		shader: PanelShader,
		shared: &Shared,
	) -> Result<Panel, AppError> {
		// Try to read the file
		let panel_path = self.panel_path(&panel_name);
		tracing::debug!(%panel_name, ?panel_path, "Loading panel");
		let panel_toml = tokio::fs::read_to_string(panel_path)
			.await
			.context("Unable to open file")?;

		// Then parse it
		let panel = toml::from_str::<ser::Panel>(&panel_toml).context("Unable to parse panel")?;

		// Finally convert it
		let geometries = panel.geometries.into_iter().map(|geometry| geometry.geometry).collect();
		let state = PanelState {
			paused:     false,
			progress:   0,
			duration:   panel.state.duration,
			fade_point: panel.state.fade_point,
		};

		let panel = Panel::new(
			panel_name.clone(),
			playlist_player,
			shared.wgpu,
			&shared.panels_renderer_layouts,
			geometries,
			state,
			shader,
		)
		.context("Unable to create panel")?;

		Ok(panel)
	}

	/// Returns a panel's path
	pub fn panel_path(&self, name: &PanelName) -> PathBuf {
		self.root.join(&*name.0).with_appended(".toml")
	}
}

/// Panel
#[derive(Debug)]
pub struct Panel {
	/// Name
	pub name: PanelName,

	/// Geometries
	pub geometries: Vec<PanelGeometry>,

	/// State
	pub state: PanelState,

	/// Images
	pub images: PanelImages,

	/// Shader
	pub shader: PanelShader,
}

impl Panel {
	/// Creates a new panel
	pub fn new(
		name: PanelName,
		playlist_player: PlaylistPlayer,
		wgpu_shared: &WgpuShared,
		renderer_layouts: &PanelsRendererLayouts,
		geometries: Vec<Rect<i32, u32>>,
		state: PanelState,
		shader: PanelShader,
	) -> Result<Self, AppError> {
		Ok(Self {
			name,
			geometries: geometries
				.into_iter()
				.map(PanelGeometry::new)
				.collect::<Result<_, _>>()
				.context("Unable to build geometries")?,
			state,
			images: PanelImages::new(playlist_player, wgpu_shared, renderer_layouts),
			shader,
		})
	}

	/// Returns the max duration for the current image
	fn max_duration(&self) -> u64 {
		match (self.images.cur().is_loaded(), self.images.next().is_loaded()) {
			(false, false) => 0,
			(true, false) => self.state.fade_point,
			(_, true) => self.state.duration,
		}
	}

	/// Skips to the next image
	pub async fn skip(
		&mut self,
		wgpu_shared: &WgpuShared,
		renderer_layouts: &PanelsRendererLayouts,
		image_requester: &ImageRequester,
	) {
		match self.images.step_next(wgpu_shared, renderer_layouts).await {
			Ok(()) => self.state.progress = self.state.duration.saturating_sub(self.state.fade_point),
			Err(()) => self.state.progress = self.max_duration(),
		}

		// Then load any missing images
		self.images
			.load_missing(wgpu_shared, renderer_layouts, image_requester, &self.geometries)
			.await;
	}

	/// Steps this panel's state by a certain number of frames (potentially negative).
	pub async fn step(
		&mut self,
		wgpu_shared: &WgpuShared,
		renderer_layouts: &PanelsRendererLayouts,
		image_requester: &ImageRequester,
		frames: i64,
	) {
		// Update the progress, potentially rolling over to the previous/next image
		self.state.progress = match self.state.progress.checked_add_signed(frames) {
			Some(next_progress) => match next_progress >= self.state.duration {
				true => match self.images.step_next(wgpu_shared, renderer_layouts).await {
					Ok(()) => next_progress.saturating_sub(self.state.duration),
					Err(()) => self.max_duration(),
				},
				false => self
					.state
					.progress
					.saturating_add_signed(frames)
					.clamp(0, self.max_duration()),
			},
			None => match self.images.step_prev(wgpu_shared, renderer_layouts).await {
				Ok(()) => self.state.duration.saturating_add_signed(frames),
				Err(()) => 0,
			},
		};

		// Then load any missing images
		self.images
			.load_missing(wgpu_shared, renderer_layouts, image_requester, &self.geometries)
			.await;
	}

	/// Updates this panel's state
	pub async fn update(
		&mut self,
		wgpu_shared: &WgpuShared,
		renderer_layouts: &PanelsRendererLayouts,
		image_requester: &ImageRequester,
	) {
		// Then load any missing images
		self.images
			.load_missing(wgpu_shared, renderer_layouts, image_requester, &self.geometries)
			.await;

		// If we're paused, don't update anything
		if self.state.paused {
			return;
		}

		self.step(wgpu_shared, renderer_layouts, image_requester, 1).await;
	}
}

/// Panel name
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Debug)]
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
