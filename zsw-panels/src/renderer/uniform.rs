//! Uniforms

// Imports
use bytemuck::{Pod, Zeroable};

/// Panel uniforms
// TODO: Be able to derive `Zeroable` and `Pod` without requiring `repr(packed)`
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[repr(C)]
pub struct PanelUniforms<X: UniformsExtra> {
	/// Position matrix
	pub pos_matrix: [[f32; 4]; 4],

	/// Front Uvs Matrix
	pub front_uvs_matrix: [[f32; 4]; 4],

	/// Back Uvs Matrix
	pub back_uvs_matrix: [[f32; 4]; 4],

	/// Front Alpha
	pub front_alpha: f32,

	/// Extra
	pub extra: X,

	/// Padding
	pub _pad: X::Pad,
}

impl<X: UniformsExtra> PanelUniforms<X> {
	/// Creates new panel uniforms
	pub fn new(
		pos_matrix: impl Into<[[f32; 4]; 4]>,
		front_uvs_matrix: impl Into<[[f32; 4]; 4]>,
		back_uvs_matrix: impl Into<[[f32; 4]; 4]>,
		front_alpha: f32,
		extra: X,
	) -> Self {
		Self {
			pos_matrix: pos_matrix.into(),
			front_uvs_matrix: front_uvs_matrix.into(),
			back_uvs_matrix: back_uvs_matrix.into(),
			front_alpha,
			extra,
			_pad: X::Pad::zeroed(),
		}
	}

	/// Returns these uniforms as bytes
	pub fn as_bytes(&self) -> &[u8] {
		// SAFETY: Transmuting to `[u8]` is never UB for `repr(C)` structs.
		//         We also guarantee `X` is Pod,
		unsafe { std::slice::from_raw_parts((self as *const Self).cast(), std::mem::size_of::<Self>()) }
	}
}

pub trait UniformsExtra {
	/// Padding type
	type Pad: Zeroable + Pod;
}


/// Fade extra
#[derive(PartialEq, Eq, Clone, Copy, Default, Debug)]
#[repr(C)]
pub struct FadeExtra {}

impl UniformsExtra for FadeExtra {
	type Pad = [u8; 3];
}

/// Fade-white extra
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[repr(C)]
pub struct FadeWhiteExtra {
	/// Strength
	pub strength: f32,
}

impl UniformsExtra for FadeWhiteExtra {
	type Pad = [u8; 2];
}

/// Fade-out extra
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[repr(C)]
pub struct FadeOutExtra {
	/// Strength
	pub strength: f32,
}

impl UniformsExtra for FadeOutExtra {
	type Pad = [u8; 2];
}

/// Fade-in extra
#[derive(PartialEq, Clone, Copy, Default, Debug)]
#[repr(C)]
pub struct FadeInExtra {
	/// Strength
	pub strength: f32,
}

impl UniformsExtra for FadeInExtra {
	type Pad = [u8; 2];
}
