//! Panel state

pub mod fade;
pub mod none;
pub mod slide;

pub use self::{fade::PanelFadeState, none::PanelNoneState, slide::PanelSlideState};

use {
	self::{fade::PanelFadeGeometryShared, none::PanelNoneGeometryShared, slide::PanelSlideGeometryShared},
	super::PanelShader,
};


/// Panel state
#[derive(Debug)]
#[expect(
	clippy::large_enum_variant,
	reason = "This enum is only stored once per panel geometry"
)]
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

/// Panel geometry
#[derive(Default, Debug)]
#[derive(zsw_util::GetOrInsert)]
pub enum PanelGeometryShared {
	#[default]
	Empty,
	None(PanelNoneGeometryShared),
	Fade(PanelFadeGeometryShared),
	Slide(PanelSlideGeometryShared),
}
