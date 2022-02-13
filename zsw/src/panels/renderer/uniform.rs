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
