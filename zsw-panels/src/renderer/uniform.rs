//! Uniforms

/// Uniforms for all panels
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[derive(bytemuck::Zeroable, bytemuck::Pod)]
#[repr(C)]
#[non_exhaustive]
pub struct PanelUniforms {
	/// Position matrix
	pub pos_matrix: [[f32; 4]; 4],

	/// Uvs Matrix
	pub uvs_matrix: [[f32; 4]; 4],

	/// Alpha
	pub alpha: f32,

	/// Extra
	// TODO: Make variable once padding works out
	pub extra: [f32; 3],
}

impl PanelUniforms {
	pub fn new(
		pos_matrix: impl Into<[[f32; 4]; 4]>,
		uvs_matrix: impl Into<[[f32; 4]; 4]>,
		alpha: f32,
		extra: impl Into<[f32; 3]>,
	) -> Self {
		Self {
			pos_matrix: pos_matrix.into(),
			uvs_matrix: uvs_matrix.into(),
			alpha,
			extra: extra.into(),
		}
	}
}
