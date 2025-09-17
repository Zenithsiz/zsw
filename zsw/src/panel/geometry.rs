//! Panel geometry

// Imports
use {
	super::state::{
		fade::{self, PanelFadeImagesShared},
		none::{self, PanelNoneShared},
		slide::{self, PanelSlideShared},
	},
	crate::panel::state::fade::PanelFadeImageSlot,
	std::collections::HashMap,
	tokio::sync::OnceCell,
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
	pub none:  OnceCell<PanelGeometryNoneUniforms>,
	pub fade:  PanelGeometryFadeUniforms,
	pub slide: OnceCell<PanelGeometrySlideUniforms>,
}

impl PanelGeometryUniforms {
	/// Returns the none uniforms
	pub async fn none(&self, wgpu: &Wgpu, shared: &PanelNoneShared) -> &PanelGeometryNoneUniforms {
		self.none
			.get_or_init(async || none::create_geometry_uniforms(wgpu, shared))
			.await
	}

	/// Returns the slide uniforms
	pub async fn slide(&self, wgpu: &Wgpu, shared: &PanelSlideShared) -> &PanelGeometrySlideUniforms {
		self.slide
			.get_or_init(async || slide::create_geometry_uniforms(wgpu, shared))
			.await
	}
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
