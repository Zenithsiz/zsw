//! Panel vertex

// Imports
use std::mem;

/// Panel vertex
#[repr(C)]
#[derive(PartialEq, Clone, Copy, Debug)]
#[derive(bytemuck::Zeroable, bytemuck::Pod)]
pub struct PanelVertex {
	/// Position
	pub pos: [f32; 2],

	/// UVs
	pub uvs: [f32; 2],
}

impl PanelVertex {
	/// The quad used to render panels with.
	pub const QUAD: [Self; 4] = [
		Self {
			pos: [-1.0, -1.0],
			uvs: [0.0, 0.0],
		},
		Self {
			pos: [1.0, -1.0],
			uvs: [1.0, 0.0],
		},
		Self {
			pos: [-1.0, 1.0],
			uvs: [0.0, 1.0],
		},
		Self {
			pos: [1.0, 1.0],
			uvs: [1.0, 1.0],
		},
	];

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
