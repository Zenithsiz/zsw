//! Panel

pub mod geometry;
mod panels;
mod renderer;
pub mod shader;

pub use self::{panels::Panels, renderer::Renderer};

use {euclid::default::Point2D, zsw_util::Rect};

/// Panel
#[derive(Debug)]
#[expect(
	clippy::large_enum_variant,
	reason = "This enum is only stored once per panel geometry"
)]
pub enum Panel {
	None(shader::none::Shader),
	Fade(shader::fade::Shader),
	Slide(shader::slide::Shader),
}

impl Panel {
	/// Returns the kind of this panel
	pub fn kind(&self) -> PanelKind {
		match self {
			Self::None(shader) => PanelKind::None(shader.kind()),
			Self::Fade(shader) => PanelKind::Fade(shader.kind()),
			Self::Slide(shader) => PanelKind::Slide(shader.kind()),
		}
	}

	/// Returns if any geometries in this panel intersects `rect`
	pub fn any_intersects(&self, rect: Rect<i32, u32>) -> bool {
		match self {
			Self::None(shader) => shader.any_intersects(rect),
			Self::Fade(shader) => shader.any_intersects(rect),
			Self::Slide(shader) => shader.any_intersects(rect),
		}
	}

	/// Returns if any geometries in this panel contain `pos`
	pub fn any_contain(&self, pos: Point2D<i32>) -> bool {
		match self {
			Self::None(shader) => shader.any_contain(pos),
			Self::Fade(shader) => shader.any_contain(pos),
			Self::Slide(shader) => shader.any_contain(pos),
		}
	}
}

/// Panel kind
#[derive(PartialEq, Clone, Copy, Debug)]
pub enum PanelKind {
	None(shader::none::Kind),
	Fade(shader::fade::Kind),
	Slide(shader::slide::Kind),
}

impl PanelKind {
	/// Returns this kind's name
	pub fn name(self) -> &'static str {
		match self {
			Self::None(kind) => kind.name(),
			Self::Fade(kind) => kind.name(),
			Self::Slide(kind) => kind.name(),
		}
	}

	/// Returns this kind's module as json
	pub fn module_json(self) -> &'static str {
		match self {
			Self::None(kind) => kind.module_json(),
			Self::Fade(kind) => kind.module_json(),
			Self::Slide(kind) => kind.module_json(),
		}
	}
}
