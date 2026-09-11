//! Panel

pub mod geometry;
mod panels;
mod renderer;
pub mod state;

pub use self::{
	geometry::PanelGeometry,
	panels::Panels,
	renderer::{PanelFadeShader, PanelShader, PanelSlideShader, PanelsRenderer},
	state::PanelState,
};

use {euclid::default::Point2D, zsw_util::Rect};

/// Panel
#[derive(Debug)]
pub struct Panel {
	/// State
	pub state: PanelState,
}

impl Panel {
	/// Returns if any geometries in this panel intersects `rect`
	pub fn any_intersects(&self, rect: Rect<i32, u32>) -> bool {
		match &self.state {
			PanelState::None(state) => state.any_intersects(rect),
			PanelState::Fade(state) => state.any_intersects(rect),
			PanelState::Slide(state) => state.any_intersects(rect),
		}
	}

	/// Returns if any geometries in this panel contain `pos`
	pub fn any_contain(&self, pos: Point2D<i32>) -> bool {
		match &self.state {
			PanelState::None(state) => state.any_contain(pos),
			PanelState::Fade(state) => state.any_contain(pos),
			PanelState::Slide(state) => state.any_contain(pos),
		}
	}
}
