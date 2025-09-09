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
use crate::{
	display::{Display, DisplayName},
	playlist::PlaylistPlayer,
};

/// Panel
#[derive(Debug)]
pub struct Panel {
	/// Display name
	pub display_name: DisplayName,

	/// Geometries
	pub geometries: Vec<PanelGeometry>,

	/// State
	pub state: PanelState,
}

impl Panel {
	/// Creates a new panel
	pub fn new(display: &Display, state: PanelState) -> Self {
		Self {
			display_name: display.name.clone(),
			geometries: display.geometries.iter().copied().map(PanelGeometry::new).collect(),
			state,
		}
	}
}
