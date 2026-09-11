//! Panel state

pub mod fade;
pub mod none;
pub mod slide;

pub use self::{
	fade::{PanelFadeKind, PanelFadeState},
	none::{PanelNoneKind, PanelNoneState},
	slide::{PanelSlideKind, PanelSlideState},
};

use self::{fade::PanelFadeGeometryShared, none::PanelNoneGeometryShared, slide::PanelSlideGeometryShared};

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
