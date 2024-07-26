//! Uniforms

// Lints
#![expect(clippy::trailing_empty_array)] // Occurs inside `derive(Pod)`


// Imports
use {
	bytemuck::{Pod, Zeroable},
	std::ptr,
};

/// `vec2<f32>`
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[repr(C, align(8))]
struct Vec2([f32; 2]);

/// `mat4x4<f32>`
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[repr(C, align(16))]
struct Matrix4x4([[f32; 4]; 4]);

/// Panel image uniforms
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[repr(C)]
pub struct PanelImageUniforms {
	/// Image ratio
	ratio: Vec2,

	/// Progress
	progress: f32,

	/// Parallax ratio
	parallax_ratio: Vec2,

	/// parallax offset
	parallax_offset: Vec2,
}

impl PanelImageUniforms {
	pub fn new(
		ratio: impl Into<[f32; 2]>,
		progress: f32,
		parallax_ratio: impl Into<[f32; 2]>,
		parallax_offset: impl Into<[f32; 2]>,
	) -> Self {
		Self {
			ratio: Vec2(ratio.into()),
			progress,
			parallax_ratio: Vec2(parallax_ratio.into()),
			parallax_offset: Vec2(parallax_offset.into()),
		}
	}
}

/// Panel uniforms
// TODO: Be able to derive `Zeroable` and `Pod` without requiring `repr(packed)`
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[repr(C)]
pub struct PanelUniforms<X: UniformsExtra> {
	/// Position matrix
	pos_matrix: Matrix4x4,

	/// Front
	front: PanelImageUniforms,

	/// Back
	back: PanelImageUniforms,

	/// Front alpha
	front_alpha: f32,

	/// Extra
	extra: X,
}

impl<X: UniformsExtra> PanelUniforms<X> {
	/// Creates new panel uniforms
	pub fn new(
		pos_matrix: impl Into<[[f32; 4]; 4]>,
		front: PanelImageUniforms,
		back: PanelImageUniforms,

		front_alpha: f32,
		extra: X,
	) -> Self {
		Self {
			pos_matrix: Matrix4x4(pos_matrix.into()),
			front,
			back,
			front_alpha,

			extra,
		}
	}

	/// Returns these uniforms as bytes
	pub fn as_bytes(&self) -> &[u8] {
		// SAFETY: Transmuting to `[u8]` is never UB for `repr(C)` structs.
		//         We also guarantee `X` is Pod,
		unsafe { std::slice::from_raw_parts(ptr::from_ref(self).cast(), std::mem::size_of::<Self>()) }
	}
}

pub trait UniformsExtra: Pod {}


/// None extra
#[derive(PartialEq, Eq, Clone, Copy, Default, Debug)]
#[derive(Zeroable, Pod)]
#[repr(C)]
pub struct NoneExtra {}

impl UniformsExtra for NoneExtra {}

/// Fade extra
#[derive(PartialEq, Eq, Clone, Copy, Default, Debug)]
#[derive(Zeroable, Pod)]
#[repr(C)]
pub struct FadeExtra {}

impl UniformsExtra for FadeExtra {}

/// Fade-white extra
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[derive(Zeroable, Pod)]
#[repr(C)]
pub struct FadeWhiteExtra {
	/// Strength
	pub strength: f32,
}

impl UniformsExtra for FadeWhiteExtra {}

/// Fade-out extra
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[derive(Zeroable, Pod)]
#[repr(C)]
pub struct FadeOutExtra {
	/// Strength
	pub strength: f32,
}

impl UniformsExtra for FadeOutExtra {}

/// Fade-in extra
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[derive(Zeroable, Pod)]
#[repr(C)]
pub struct FadeInExtra {
	/// Strength
	pub strength: f32,
}

impl UniformsExtra for FadeInExtra {}
