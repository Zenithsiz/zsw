//! Fade uniforms

// Imports
use {
	super::{Matrix4x4, Vec2},
	bytemuck::{Pod, Zeroable},
};

/// Fade
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[derive(Zeroable, Pod)]
#[repr(C)]
pub struct Basic {
	pub pos_matrix:  Matrix4x4,
	pub image_ratio: Vec2,
	pub progress:    f32,
	pub alpha:       f32,
}

/// Fade-out
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[derive(Zeroable, Pod)]
#[repr(C)]
pub struct Out {
	pub pos_matrix:    Matrix4x4,
	pub image_ratio:   Vec2,
	pub progress:      f32,
	pub alpha:         f32,
	pub strength:      f32,
	pub fade_progress: f32,

	pub _unused: [u32; 2],
}
