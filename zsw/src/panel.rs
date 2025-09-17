//! Panel

// Modules
mod panels;
mod renderer;
pub mod state;

// Exports
pub use self::{
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

	/// State
	pub state: PanelState,
}

impl Panel {
	/// Creates a new panel
	pub fn new(display: Arc<RwLock<Display>>, state: PanelState) -> Self {
		Self { display, state }
	}
}
