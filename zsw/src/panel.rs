//! Panel

// Modules
mod geometry;
mod images;
mod renderer;
mod state;

// Exports
pub use self::{
	geometry::{PanelGeometry, PanelGeometryUniforms},
	images::{PanelFadeImage, PanelFadeImages},
	renderer::{PanelFadeShader, PanelShader, PanelsRenderer, PanelsRendererShared},
	state::{PanelFadeState, PanelNoneState, PanelState},
};

// Imports
use {
	crate::{display::Display, playlist::PlaylistPlayer},
	futures::lock::Mutex,
	std::sync::Arc,
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
