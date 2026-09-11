//! Panel none state

use {
	crate::panel::{PanelGeometry, renderer::uniform},
	euclid::default::Transform3D,
	std::sync::OnceLock,
	zsw_wgpu::WgpuRenderer,
};

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

	/// Renders a geometry of this panel
	pub fn render(
		&self,
		shared: &PanelNoneShared,
		wgpu_renderer: &WgpuRenderer,
		render_pass: &mut wgpu::RenderPass<'_>,
		panel_geometry: &mut PanelGeometry,
		pos_matrix: Transform3D<f32>,
	) {
		let geometry_uniforms = panel_geometry
			.shared
			.none_or_insert_default()
			.uniforms(wgpu_renderer, shared);

		wgpu_renderer
			.shared
			.write_buffer(&geometry_uniforms.buffer, &uniform::None {
				pos_matrix:       uniform::Matrix4x4(pos_matrix.to_arrays()),
				background_color: uniform::Vec4(self.background_color),
			});

		// Bind the geometry uniforms
		render_pass.set_bind_group(0, &geometry_uniforms.bind_group, &[]);

		render_pass.draw_indexed(0..6, 0, 0..1);
	}
}

/// Panel none geometry shared
#[derive(Default, Debug)]
pub struct PanelNoneGeometryShared {
	/// Uniforms
	pub uniforms: Option<PanelNoneGeometryUniforms>,
}

impl PanelNoneGeometryShared {
	/// Returns this geometry's uniforms
	pub fn uniforms(
		&mut self,
		wgpu_renderer: &WgpuRenderer,
		shared: &PanelNoneShared,
	) -> &mut PanelNoneGeometryUniforms {
		self.uniforms
			.get_or_insert_with(|| self::create_geometry_uniforms(wgpu_renderer, shared))
	}
}

/// Panel none shared
#[derive(Debug)]
pub struct PanelNoneShared {
	/// Geometry uniforms bind group layout
	pub geometry_uniforms_bind_group_layout: OnceLock<wgpu::BindGroupLayout>,
}

impl PanelNoneShared {
	/// Creates the shared
	pub fn new() -> Self {
		Self {
			geometry_uniforms_bind_group_layout: OnceLock::new(),
		}
	}

	pub fn geometry_uniforms_bind_group_layout(&self, wgpu_renderer: &WgpuRenderer) -> &wgpu::BindGroupLayout {
		self.geometry_uniforms_bind_group_layout
			.get_or_init(|| self::create_geometry_uniforms_bind_group_layout(wgpu_renderer))
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
fn create_geometry_uniforms_bind_group_layout(wgpu_renderer: &WgpuRenderer) -> wgpu::BindGroupLayout {
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

	wgpu_renderer.shared.device.create_bind_group_layout(&descriptor)
}

/// Creates the panel none geometry uniforms
fn create_geometry_uniforms(wgpu_renderer: &WgpuRenderer, shared: &PanelNoneShared) -> PanelNoneGeometryUniforms {
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
	let buffer = wgpu_renderer.shared.device.create_buffer(&buffer_descriptor);

	// Create the uniform bind group
	let bind_group_descriptor = wgpu::BindGroupDescriptor {
		label:   Some("zsw-panel-none-geometry-uniforms-bind-group"),
		layout:  shared.geometry_uniforms_bind_group_layout(wgpu_renderer),
		entries: &[wgpu::BindGroupEntry {
			binding:  0,
			resource: buffer.as_entire_binding(),
		}],
	};
	let bind_group = wgpu_renderer.shared.device.create_bind_group(&bind_group_descriptor);

	PanelNoneGeometryUniforms { buffer, bind_group }
}
