//! Panel state

pub mod fade;
pub mod none;
pub mod slide;

pub use self::{
	fade::{PanelFadeShader, PanelFadeState},
	none::{PanelNoneShader, PanelNoneState},
	slide::{PanelSlideShader, PanelSlideState},
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
