//! Panel state

pub mod fade;
pub mod none;
pub mod slide;

pub use self::{
	fade::{PanelFadeKind, PanelFadeState},
	none::{PanelNoneKind, PanelNoneState},
	slide::{PanelSlideKind, PanelSlideState},
};
