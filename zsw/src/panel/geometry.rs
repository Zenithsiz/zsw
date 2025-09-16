//! Panel geometry

// Imports
use {std::collections::HashMap, winit::window::WindowId};


/// Panel geometry
#[derive(Debug)]
pub struct PanelGeometry {
	/// Uniforms
	pub uniforms: HashMap<WindowId, PanelGeometryUniforms>,
}

/// Panel geometry uniforms
#[derive(Debug)]
pub struct PanelGeometryUniforms {
	/// Buffer
	pub buffer: wgpu::Buffer,

	/// Bind group
	pub bind_group: wgpu::BindGroup,
}

impl PanelGeometry {
	pub fn new() -> Self {
		Self {
			uniforms: HashMap::new(),
		}
	}
}
