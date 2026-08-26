//! Panel state

// Modules
pub mod fade;
pub mod none;
pub mod slide;

// Exports
pub use self::{fade::PanelFadeState, none::PanelNoneState, slide::PanelSlideState};

// Imports
use {
	self::{fade::PanelFadeGeometryShared, none::PanelNoneGeometryShared, slide::PanelSlideGeometryShared},
	super::PanelShader,
};


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

/// Panel geometry
#[derive(Default, Debug)]
pub enum PanelGeometryShared {
	#[default]
	Empty,

	/// None shader
	None(PanelNoneGeometryShared),

	/// Fade shader
	Fade(PanelFadeGeometryShared),

	/// Slide shader
	Slide(PanelSlideGeometryShared),
}

// TODO: Reduce repetition of this?
impl PanelGeometryShared {
	/// Gets the none shader of this geometry
	pub fn none(&mut self) -> &mut PanelNoneGeometryShared {
		if let Self::None(shared) = self {
			return shared;
		}

		*self = Self::None(PanelNoneGeometryShared::default());
		let Self::None(shared) = self else { unreachable!() };
		shared
	}

	/// Gets the fade shader of this geometry
	pub fn fade(&mut self) -> &mut PanelFadeGeometryShared {
		if let Self::Fade(shared) = self {
			return shared;
		}

		*self = Self::Fade(PanelFadeGeometryShared::default());
		let Self::Fade(shared) = self else { unreachable!() };
		shared
	}

	/// Gets the slide shader of this geometry
	pub fn slide(&mut self) -> &mut PanelSlideGeometryShared {
		if let Self::Slide(shared) = self {
			return shared;
		}

		*self = Self::Slide(PanelSlideGeometryShared::default());
		let Self::Slide(shared) = self else { unreachable!() };
		shared
	}
}
