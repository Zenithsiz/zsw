//! Panel

// Modules
mod geometry;
mod images;
mod panels;
mod renderer;
mod state;

// Exports
pub use self::{
	geometry::{PanelGeometry, PanelGeometryUniforms},
	images::{PanelFadeImage, PanelFadeImages},
	panels::Panels,
	renderer::{PanelFadeShader, PanelShader, PanelSlideShader, PanelsRenderer, PanelsRendererShared},
	state::{PanelFadeState, PanelNoneState, PanelSlideState, PanelState},
};

// Imports
use {
	crate::{display::Display, playlist::PlaylistPlayer},
	std::sync::Arc,
	tokio::sync::Mutex,
};

/// Panel
#[derive(Debug)]
pub struct Panel {
	/// Display
	pub display: Arc<Mutex<Display>>,

	/// Geometries
	pub geometries: Vec<PanelGeometry>,

	/// State
	pub state: PanelState,
}

impl Panel {
	/// Creates a new panel
	pub fn new(display: Arc<Mutex<Display>>, state: PanelState) -> Self {
		Self {
			display,
			geometries: vec![],
			state,
		}
	}
}
