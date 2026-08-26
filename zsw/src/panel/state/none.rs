//! Panel none state

// Imports
use {crate::panel::renderer::uniform, std::collections::HashMap, winit::window::WindowId, zsw_wgpu::Wgpu};

/// Panel none state
#[derive(Debug)]
pub struct PanelNoneState {
	/// Background color
	pub background_color: [f32; 4],
}

impl PanelNoneState {
	/// Creates new state
	pub fn new(background_color: [f32; 4]) -> Self {
		Self { background_color }
	}
}

/// Panel none geometry shared
#[derive(Default, Debug)]
pub struct PanelNoneGeometryShared {
	/// uniforms
	pub uniforms: HashMap<WindowId, PanelNoneGeometryUniforms>,
}

impl PanelNoneGeometryShared {
	/// Returns this geometry's uniforms
	pub fn uniforms(
		&mut self,
		wgpu: &Wgpu,
		shared: &PanelNoneShared,
		window_id: WindowId,
	) -> &mut PanelNoneGeometryUniforms {
		self.uniforms
			.entry(window_id)
			.or_insert_with(|| self::create_geometry_uniforms(wgpu, shared))
	}
}

/// Panel none shared
#[derive(Debug)]
pub struct PanelNoneShared {
	/// Geometry uniforms bind group layout
	pub geometry_uniforms_bind_group_layout: wgpu::BindGroupLayout,
}

impl PanelNoneShared {
	/// Creates the shared
	pub fn new(wgpu: &Wgpu) -> Self {
		let geometry_uniforms_bind_group_layout = self::create_geometry_uniforms_bind_group_layout(wgpu);

		Self {
			geometry_uniforms_bind_group_layout,
		}
	}
}

/// Panel geometry none uniforms
#[derive(Debug)]
pub struct PanelNoneGeometryUniforms {
	/// Buffer
	pub buffer: wgpu::Buffer,

	/// Bind group
	pub bind_group: wgpu::BindGroup,
}

/// Creates the geometry uniforms bind group layout
fn create_geometry_uniforms_bind_group_layout(wgpu: &Wgpu) -> wgpu::BindGroupLayout {
	let descriptor = wgpu::BindGroupLayoutDescriptor {
		label:   Some("zsw-panel-none-geometry-uniforms-bind-group-layout"),
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
fn create_geometry_uniforms(wgpu: &Wgpu, shared: &PanelNoneShared) -> PanelNoneGeometryUniforms {
	// Create the uniforms
	let buffer_descriptor = wgpu::BufferDescriptor {
		label:              Some("zsw-panel-none-geometry-uniforms-buffer"),
		usage:              wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
		size:               u64::try_from(
			zsw_util::array_max(&[size_of::<uniform::None>()]).expect("No max uniform size"),
		)
		.expect("Maximum uniform size didn't fit into a `u64`"),
		mapped_at_creation: false,
	};
	let buffer = wgpu.device.create_buffer(&buffer_descriptor);

	// Create the uniform bind group
	let bind_group_descriptor = wgpu::BindGroupDescriptor {
		label:   Some("zsw-panel-none-geometry-uniforms-bind-group"),
		layout:  &shared.geometry_uniforms_bind_group_layout,
		entries: &[wgpu::BindGroupEntry {
			binding:  0,
			resource: buffer.as_entire_binding(),
		}],
	};
	let bind_group = wgpu.device.create_bind_group(&bind_group_descriptor);

	PanelNoneGeometryUniforms { buffer, bind_group }
}
