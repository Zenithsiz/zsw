//! Fade uniforms

// Imports
use {
	super::{Matrix4x4, Vec2},
	bytemuck::{Pod, Zeroable},
};

/// Image
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[derive(Zeroable, Pod)]
#[repr(C)]
pub struct Image {
	pub image_ratio: Vec2,
	pub progress:    f32,
	pub alpha:       f32,
}

/// Images
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[derive(Zeroable, Pod)]
#[repr(C)]
pub struct Images {
	pub prev: Image,
	pub cur:  Image,
	pub next: Image,
}

/// Fade
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[derive(Zeroable, Pod)]
#[repr(C)]
pub struct Basic {
	pub pos_matrix: Matrix4x4,
	pub images:     Images,

	pub _unused: [u32; 4],
}

/// Fade-out
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[derive(Zeroable, Pod)]
#[repr(C)]
pub struct Out {
	pub pos_matrix: Matrix4x4,
	pub images:     Images,
	pub strength:   f32,

	pub _unused: [u32; 3],
}
