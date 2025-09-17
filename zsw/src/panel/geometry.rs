//! Panel geometry

// Imports
use {
	super::state::fade::{self, PanelFadeImagesShared},
	crate::panel::state::fade::PanelFadeImageSlot,
	std::collections::HashMap,
	winit::window::WindowId,
	zsw_wgpu::Wgpu,
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
	pub fade: PanelGeometryFadeUniforms,
}

/// Panel geometry fade uniforms
#[derive(Default, Debug)]
pub struct PanelGeometryFadeUniforms {
	/// Images
	pub images: HashMap<PanelFadeImageSlot, PanelGeometryFadeImageUniforms>,
}
impl PanelGeometryFadeUniforms {
	/// Returns an image's uniforms
	pub fn image(
		&mut self,
		wgpu: &Wgpu,
		shared: &PanelFadeImagesShared,
		slot: PanelFadeImageSlot,
	) -> &mut PanelGeometryFadeImageUniforms {
		self.images
			.entry(slot)
			.or_insert_with(|| fade::images::create_image_geometry_uniforms(wgpu, shared))
	}
}

/// Panel geometry fade image uniforms
#[derive(Debug)]
pub struct PanelGeometryFadeImageUniforms {
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
