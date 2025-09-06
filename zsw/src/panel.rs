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
	geometry::{PanelGeometry, PanelGeometryUniforms},
	images::{PanelImage, PanelImages},
	panels::Panels,
	renderer::{PanelShader, PanelShaderFade, PanelsRenderer, PanelsRendererLayouts},
	state::PanelState,
};

// Imports
use {
	crate::playlist::PlaylistPlayer,
	chrono::TimeDelta,
	core::{borrow::Borrow, fmt},
	std::sync::Arc,
	zsw_util::Rect,
	zsw_wgpu::WgpuShared,
};

/// Panel
#[derive(Debug)]
pub struct Panel {
	/// Name
	name: PanelName,

	/// Geometries
	geometries: Vec<PanelGeometry>,

	/// Playlist player
	playlist_player: Option<PlaylistPlayer>,

	/// State
	state: PanelState,
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
	pub fn skip(&mut self, wgpu_shared: &WgpuShared) {
		let Some(playlist_player) = &mut self.playlist_player else {
			return;
		};

		self.state.skip(playlist_player, wgpu_shared);
	}

	/// Steps this panel's state by a certain number of frames (potentially negative).
	///
	/// If the playlist player isn't loaded, does nothing
	pub fn step(&mut self, wgpu_shared: &WgpuShared, delta: TimeDelta) {
		let Some(playlist_player) = &mut self.playlist_player else {
			return;
		};

		self.state.step(playlist_player, wgpu_shared, delta);
	}

	/// Updates this panel's state using the current time as a delta
	///
	/// If the playlist player isn't loaded, does nothing
	pub fn update(&mut self, wgpu_shared: &WgpuShared) {
		let Some(playlist_player) = &mut self.playlist_player else {
			return;
		};

		self.state.update(playlist_player, wgpu_shared);
	}

	/// Returns this panel's name
	pub fn name(&self) -> &PanelName {
		&self.name
	}

	/// Returns this panel's geometries
	pub fn geometries(&self) -> &[PanelGeometry] {
		&self.geometries
	}

	/// Returns this panel's geometries mutably
	pub fn geometries_mut(&mut self) -> &mut Vec<PanelGeometry> {
		&mut self.geometries
	}

	/// Returns this panel's playlist player
	pub fn playlist_player(&self) -> Option<&PlaylistPlayer> {
		self.playlist_player.as_ref()
	}

	/// Sets this panel's playlist player
	pub fn set_playlist_player(&mut self, playlist_player: PlaylistPlayer) {
		self.playlist_player = Some(playlist_player);
	}

	/// Returns this panel's state
	pub fn state(&self) -> &PanelState {
		&self.state
	}

	/// Returns this panel's state mutably
	pub fn state_mut(&mut self) -> &mut PanelState {
		&mut self.state
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
