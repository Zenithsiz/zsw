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
	core::{borrow::Borrow, fmt},
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

		self.state.skip(playlist_player, wgpu_shared, renderer_layouts);
	}

	/// Steps this panel's state by a certain number of frames (potentially negative).
	///
	/// If the playlist player isn't loaded, does nothing
	pub fn step(&mut self, wgpu_shared: &WgpuShared, renderer_layouts: &PanelsRendererLayouts, delta: TimeDelta) {
		let Some(playlist_player) = &mut self.playlist_player else {
			return;
		};

		self.state.step(playlist_player, wgpu_shared, renderer_layouts, delta);
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

		self.state.update(playlist_player, wgpu_shared, renderer_layouts, delta);
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
