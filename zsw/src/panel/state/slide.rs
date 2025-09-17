//! Panel slide state

// Imports
use {
	crate::panel::{PanelSlideShader, geometry::PanelGeometrySlideUniforms, renderer::uniform},
	zsw_wgpu::Wgpu,
};

/// Panel slide state
#[derive(Debug)]
pub struct PanelSlideState {
	/// Shader
	shader: PanelSlideShader,
}

impl PanelSlideState {
	/// Creates new state
	pub fn new(shader: PanelSlideShader) -> Self {
		Self { shader }
	}

	/// Returns the panel shader
	pub fn shader(&self) -> PanelSlideShader {
		self.shader
	}

	/// Returns the panel shader mutably
	pub fn shader_mut(&mut self) -> &mut PanelSlideShader {
		&mut self.shader
	}
}

/// Panel slide images shared
#[derive(Debug)]
pub struct PanelSlideImagesShared {
	/// Geometry uniforms bind group layout
	pub geometry_uniforms_bind_group_layout: wgpu::BindGroupLayout,
}

impl PanelSlideImagesShared {
	/// Creates the shared
	pub fn new(wgpu: &Wgpu) -> Self {
		let geometry_uniforms_bind_group_layout = self::create_geometry_uniforms_bind_group_layout(wgpu);

		Self {
			geometry_uniforms_bind_group_layout,
		}
	}
}

/// Creates the geometry uniforms bind group layout
fn create_geometry_uniforms_bind_group_layout(wgpu: &Wgpu) -> wgpu::BindGroupLayout {
	let descriptor = wgpu::BindGroupLayoutDescriptor {
		label:   Some("zsw-panel-slide-geometry-uniforms-bind-group-layout"),
		entries: &[wgpu::BindGroupLayoutEntry {
			binding:    0,
			visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
			ty:         wgpu::BindingType::Buffer {
				ty:                 wgpu::BufferBindingType::Uniform,
				has_dynamic_offset: false,
				min_binding_size:   None,
			},
			count:      None,
		}],
	};

	wgpu.device.create_bind_group_layout(&descriptor)
}

/// Creates the panel none geometry uniforms
pub fn create_geometry_uniforms(wgpu: &Wgpu, layout: &wgpu::BindGroupLayout) -> PanelGeometrySlideUniforms {
	// Create the uniforms
	let buffer_descriptor = wgpu::BufferDescriptor {
		label:              Some("zsw-panel-none-geometry-uniforms-buffer"),
		usage:              wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
		size:               u64::try_from(
			zsw_util::array_max(&[size_of::<uniform::Slide>()]).expect("No max uniform size"),
		)
		.expect("Maximum uniform size didn't fit into a `u64`"),
		mapped_at_creation: false,
	};
	let buffer = wgpu.device.create_buffer(&buffer_descriptor);

	// Create the uniform bind group
	let bind_group_descriptor = wgpu::BindGroupDescriptor {
		label: Some("zsw-panel-none-geometry-uniforms-bind-group"),
		layout,
		entries: &[wgpu::BindGroupEntry {
			binding:  0,
			resource: buffer.as_entire_binding(),
		}],
	};
	let bind_group = wgpu.device.create_bind_group(&bind_group_descriptor);

	PanelGeometrySlideUniforms { buffer, bind_group }
}
