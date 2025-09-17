//! Uniforms

// Imports
use bytemuck::{Pod, Zeroable};

/// `vec2<f32>`
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[derive(Zeroable, Pod)]
#[repr(C, align(8))]
pub struct Vec2(pub [f32; 2]);

/// `vec4<f32>`
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[derive(Zeroable, Pod)]
#[repr(C, align(16))]
pub struct Vec4(pub [f32; 4]);

/// `mat4x4<f32>`
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[derive(Zeroable, Pod)]
#[repr(C, align(16))]
pub struct Matrix4x4(pub [[f32; 4]; 4]);

/// None
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[derive(Zeroable, Pod)]
#[repr(C)]
pub struct None {
	pub pos_matrix:       Matrix4x4,
	pub background_color: Vec4,
}

/// Fade
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[derive(Zeroable, Pod)]
#[repr(C)]
pub struct FadeBasic {
	pub pos_matrix:  Matrix4x4,
	pub image_ratio: Vec2,
	pub progress:    f32,
	pub alpha:       f32,
}

/// Fade-white
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[derive(Zeroable, Pod)]
#[repr(C)]
pub struct FadeWhite {
	pub pos_matrix:   Matrix4x4,
	pub image_ratio:  Vec2,
	pub progress:     f32,
	pub alpha:        f32,
	pub mix_strength: f32,

	pub _unused: [u32; 3],
}

/// Fade-out
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[derive(Zeroable, Pod)]
#[repr(C)]
pub struct FadeOut {
	pub pos_matrix:  Matrix4x4,
	pub image_ratio: Vec2,
	pub progress:    f32,
	pub alpha:       f32,
	pub strength:    f32,

	pub _unused: [u32; 3],
}

/// Fade-in
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[derive(Zeroable, Pod)]
#[repr(C)]
pub struct FadeIn {
	pub pos_matrix:  Matrix4x4,
	pub image_ratio: Vec2,
	pub progress:    f32,
	pub alpha:       f32,
	pub strength:    f32,

	pub _unused: [u32; 3],
}

/// Slide
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[derive(Zeroable, Pod)]
#[repr(C)]
pub struct Slide {
	pub pos_matrix: Matrix4x4,
}
