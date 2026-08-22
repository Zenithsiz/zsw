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
use crate::display::DisplayName;

/// Panel
#[derive(Debug)]
pub struct Panel {
	/// Display name
	pub display_name: DisplayName,

	/// State
	pub state: PanelState,
}

impl Panel {
	/// Creates a new panel
	pub fn new(display_name: DisplayName, state: PanelState) -> Self {
		Self { display_name, state }
	}
}
