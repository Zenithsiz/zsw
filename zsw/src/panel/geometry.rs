//! Panel geometry

// Imports
use {
	crate::panel::state::fade::PanelFadeImageSlot,
	std::collections::HashMap,
	tokio::sync::OnceCell,
	winit::window::WindowId,
};

/// Panel geometry
#[derive(Debug)]
pub struct PanelGeometry {
	/// Uniforms
	pub uniforms: HashMap<WindowId, PanelGeometryUniforms>,
}

/// Panel geometry uniforms
#[derive(Default, Debug)]
pub struct PanelGeometryUniforms {
	pub none:  OnceCell<PanelGeometryNoneUniforms>,
	pub fade:  PanelGeometryFadeUniforms,
	pub slide: OnceCell<PanelGeometrySlideUniforms>,
}

/// Panel geometry none uniforms
#[derive(Debug)]
pub struct PanelGeometryNoneUniforms {
	/// Buffer
	pub buffer: wgpu::Buffer,

	/// Bind group
	pub bind_group: wgpu::BindGroup,
}

/// Panel geometry fade uniforms
#[derive(Default, Debug)]
pub struct PanelGeometryFadeUniforms {
	/// Images
	pub images: HashMap<PanelFadeImageSlot, PanelGeometryFadeImageUniforms>,
}

/// Panel geometry fade image uniforms
#[derive(Debug)]
pub struct PanelGeometryFadeImageUniforms {
	/// Buffer
	pub buffer: wgpu::Buffer,

	/// Bind group
	pub bind_group: wgpu::BindGroup,
}

/// Panel geometry slide uniforms
#[derive(Debug)]
pub struct PanelGeometrySlideUniforms {
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
