//! Panel state

// Modules
pub mod fade;
pub mod none;
pub mod slide;

// Exports
pub use self::{fade::PanelFadeState, none::PanelNoneState, slide::PanelSlideState};

// Imports
use super::PanelShader;


/// Panel state
#[derive(Debug)]
#[expect(clippy::large_enum_variant, reason = "Indirections are more costly")]
pub enum PanelState {
	/// None shader
	None(PanelNoneState),

	/// Fade shader
	Fade(PanelFadeState),

	/// Slide shader
	Slide(PanelSlideState),
}

impl PanelState {
	/// Returns the shader of this state
	pub fn shader(&self) -> PanelShader {
		match self {
			Self::None(state) => PanelShader::None {
				background_color: state.background_color,
			},
			Self::Fade(state) => PanelShader::Fade(state.shader()),
			Self::Slide(state) => PanelShader::Slide(state.shader()),
		}
	}
}
