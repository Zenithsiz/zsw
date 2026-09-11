//! Panel

pub mod geometry;
mod panels;
mod renderer;
pub mod state;

pub use self::{
	geometry::PanelGeometry,
	panels::Panels,
	renderer::{PanelShader, PanelsRenderer},
};

use {
	self::state::{PanelFadeState, PanelNoneState, PanelSlideState},
	euclid::default::Point2D,
	zsw_util::Rect,
};

/// Panel
#[derive(Debug)]
#[expect(
	clippy::large_enum_variant,
	reason = "This enum is only stored once per panel geometry"
)]
pub enum Panel {
	/// None shader
	None(PanelNoneState),

	/// Fade shader
	Fade(PanelFadeState),

	/// Slide shader
	Slide(PanelSlideState),
}

impl Panel {
	/// Returns the shader of this panel
	pub fn shader(&self) -> PanelShader {
		match self {
			Self::None(state) => PanelShader::None(state.shader()),
			Self::Fade(state) => PanelShader::Fade(state.shader()),
			Self::Slide(state) => PanelShader::Slide(state.shader()),
		}
	}

	/// Returns if any geometries in this panel intersects `rect`
	pub fn any_intersects(&self, rect: Rect<i32, u32>) -> bool {
		match self {
			Self::None(state) => state.any_intersects(rect),
			Self::Fade(state) => state.any_intersects(rect),
			Self::Slide(state) => state.any_intersects(rect),
		}
	}

	/// Returns if any geometries in this panel contain `pos`
	pub fn any_contain(&self, pos: Point2D<i32>) -> bool {
		match self {
			Self::None(state) => state.any_contain(pos),
			Self::Fade(state) => state.any_contain(pos),
			Self::Slide(state) => state.any_contain(pos),
		}
	}
}
