//! Uniforms

// Imports
use bytemuck::{Pod, Zeroable};

/// `vec2<f32>`
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[derive(Zeroable, Pod)]
#[repr(C, align(8))]
pub struct Vec2(pub [f32; 2]);

/// `mat4x4<f32>`
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[derive(Zeroable, Pod)]
#[repr(C, align(16))]
pub struct Matrix4x4(pub [[f32; 4]; 4]);

/// Panel image uniforms
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[derive(Zeroable, Pod)]
#[repr(C)]
pub struct PanelImageUniforms {
	ratio:    Vec2,
	swap_dir: u32,
	_unused:  u32,
}

impl PanelImageUniforms {
	pub fn new(ratio: impl Into<[f32; 2]>, swap_dir: bool) -> Self {
		Self {
			ratio:    Vec2(ratio.into()),
			swap_dir: swap_dir.into(),
			_unused:  0,
		}
	}
}

/// None
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[derive(Zeroable, Pod)]
#[repr(C)]
pub struct None {
	pub pos_matrix: Matrix4x4,
}

/// Fade
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[derive(Zeroable, Pod)]
#[repr(C)]
pub struct Fade {
	pub pos_matrix: Matrix4x4,
	pub prev:       PanelImageUniforms,
	pub cur:        PanelImageUniforms,
	pub next:       PanelImageUniforms,
	pub fade_point: f32,
	pub progress:   f32,

	pub _unused: [u32; 2],
}

/// Fade-white
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[derive(Zeroable, Pod)]
#[repr(C)]
pub struct FadeWhite {
	pub pos_matrix: Matrix4x4,
	pub prev:       PanelImageUniforms,
	pub cur:        PanelImageUniforms,
	pub next:       PanelImageUniforms,
	pub fade_point: f32,
	pub progress:   f32,
	pub strength:   f32,

	pub _unused: u32,
}

/// Fade-out
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[derive(Zeroable, Pod)]
#[repr(C)]
pub struct FadeOut {
	pub pos_matrix: Matrix4x4,
	pub prev:       PanelImageUniforms,
	pub cur:        PanelImageUniforms,
	pub next:       PanelImageUniforms,
	pub fade_point: f32,
	pub progress:   f32,
	pub strength:   f32,

	pub _unused: u32,
}

/// Fade-in
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[derive(Zeroable, Pod)]
#[repr(C)]
pub struct FadeIn {
	pub pos_matrix: Matrix4x4,
	pub prev:       PanelImageUniforms,
	pub cur:        PanelImageUniforms,
	pub next:       PanelImageUniforms,
	pub fade_point: f32,
	pub progress:   f32,
	pub strength:   f32,

	pub _unused: u32,
}
