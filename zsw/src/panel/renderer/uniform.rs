//! Uniforms

pub mod fade;

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

/// Slide
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[derive(Zeroable, Pod)]
#[repr(C)]
pub struct Slide {
	pub pos_matrix: Matrix4x4,
}
