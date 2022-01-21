//! Uniforms

/// Uniforms for all panels
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[derive(bytemuck::Zeroable, bytemuck::Pod)]
#[repr(C)]
pub struct PanelUniforms {
	/// Matrix
	pub matrix: [[f32; 4]; 4],

	/// Uvs start
	pub uvs_start: [f32; 2],

	/// Uvs offset
	pub uvs_offset: [f32; 2],

	/// Alpha
	pub alpha: f32,

	pub _pad: [f32; 3],
}

impl PanelUniforms {
	/// Creates new uniforms
	// TODO: Do this properly
	pub const fn new() -> Self {
		Self {
			matrix:     [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [
				0.0, 0.0, 0.0, 1.0,
			]],
			uvs_start:  [0.0, 0.0],
			uvs_offset: [0.0, 0.0],
			alpha:      1.0,
			_pad:       [0.0; 3],
		}
	}
}
