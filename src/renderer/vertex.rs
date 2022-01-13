//! Vertices

// Imports
use std::mem;

/// Vertex
#[repr(C)]
#[derive(PartialEq, Clone, Copy, Debug)]
#[derive(bytemuck::Zeroable, bytemuck::Pod)]
pub struct Vertex {
	/// Position
	pub pos: [f32; 2],

	/// UVs
	pub uvs: [f32; 2],
}

impl Vertex {
	/// Returns the buffer layout of this vertex
	#[must_use]
	pub const fn buffer_layout() -> wgpu::VertexBufferLayout<'static> {
		wgpu::VertexBufferLayout {
			array_stride: mem::size_of::<Self>() as wgpu::BufferAddress,
			step_mode:    wgpu::VertexStepMode::Vertex,
			attributes:   &[
				wgpu::VertexAttribute {
					offset:          0,
					format:          wgpu::VertexFormat::Float32x2,
					shader_location: 0,
				},
				wgpu::VertexAttribute {
					offset:          mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
					format:          wgpu::VertexFormat::Float32x2,
					shader_location: 1,
				},
			],
		}
	}
}
