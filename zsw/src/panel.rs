//! Panel

mod geometry;
mod panels;
mod renderer;
pub mod state;

pub use self::{
	geometry::PanelGeometry,
	panels::Panels,
	renderer::{PanelFadeShader, PanelShader, PanelSlideShader, PanelsRenderer},
	state::PanelState,
};

/// Panel
#[derive(Debug)]
pub struct Panel {
	/// Geometries
	pub geometries: Vec<PanelGeometry>,

	/// State
	pub state: PanelState,
}
