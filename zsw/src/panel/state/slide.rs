//! Panel slide state

use crate::panel::PanelSlideShader;

/// Panel slide state
#[derive(Debug)]
pub struct PanelSlideState {
	/// Shader
	shader: PanelSlideShader,
}

impl PanelSlideState {
	/// Creates new state
	pub fn new(shader: PanelSlideShader) -> Self {
		Self { shader }
	}

	/// Returns the panel shader
	pub fn shader(&self) -> PanelSlideShader {
		self.shader
	}

	/// Returns the panel shader mutably
	pub fn shader_mut(&mut self) -> &mut PanelSlideShader {
		&mut self.shader
	}
}
