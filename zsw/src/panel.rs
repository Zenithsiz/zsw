//! Panel

// Modules
mod geometry;
mod panels;
mod renderer;
pub mod state;

// Exports
pub use self::{
	geometry::PanelGeometry,
	panels::Panels,
	renderer::{PanelFadeShader, PanelShader, PanelSlideShader, PanelsRenderer, PanelsRendererShared},
	state::PanelState,
};

// Imports
use {crate::display::Display, std::sync::Arc, tokio::sync::RwLock};

/// Panel
#[derive(Debug)]
pub struct Panel {
	/// Display
	pub display: Arc<RwLock<Display>>,

	/// Geometries
	pub geometries: Vec<PanelGeometry>,

	/// State
	pub state: PanelState,
}

impl Panel {
	/// Creates a new panel
	pub fn new(display: Arc<RwLock<Display>>, state: PanelState) -> Self {
		Self {
			display,
			geometries: vec![],
			state,
		}
	}
}
