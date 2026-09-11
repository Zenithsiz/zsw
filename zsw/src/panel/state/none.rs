//! Panel none state

use {
	crate::panel::{PanelGeometry, geometry, renderer::uniform},
	euclid::default::Point2D,
	std::sync::OnceLock,
	zsw_util::Rect,
	zsw_wgpu::Wgpu,
};

/// Panel none state
#[derive(Debug)]
pub struct PanelNoneState {
	/// Geometries
	geometries: Vec<PanelGeometry>,

	/// Background color
	background_color: [f32; 4],

	/// Shader
	shader: PanelNoneShader,
}

impl PanelNoneState {
	/// Creates new state
	pub fn new(geometries: Vec<PanelGeometry>, background_color: [f32; 4]) -> Self {
		Self {
			geometries,
			background_color,
			shader: PanelNoneShader::Basic,
		}
	}

	/// Returns the shader of this state
	pub fn shader(&self) -> PanelNoneShader {
		self.shader
	}

	/// Returns if any geometries in this panel intersects `rect`
	pub fn any_intersects(&self, rect: Rect<i32, u32>) -> bool {
		self.geometries.iter().any(|geometry| geometry.rect.intersects(rect))
	}

	/// Returns if any geometries in this panel contain `pos`
	pub fn any_contain(&self, pos: Point2D<i32>) -> bool {
		self.geometries.iter().any(|geometry| geometry.rect.contains(pos))
	}

	/// Renders a geometry of this panel
	pub fn render(
		&mut self,
		shared: &PanelNoneShared,
		wgpu: &Wgpu,
		surface_geometry: Rect<i32, u32>,
		render_pass: &mut wgpu::RenderPass<'_>,
	) {
		for panel_geometry in &mut self.geometries {
			let geometry_uniforms = panel_geometry.shared.none_or_insert_default().uniforms(wgpu, shared);

			let pos_matrix = geometry::pos_matrix(panel_geometry.rect, surface_geometry);
			wgpu.write_buffer(&geometry_uniforms.buffer, &uniform::None {
				pos_matrix:       uniform::Matrix4x4(pos_matrix.to_arrays()),
				background_color: uniform::Vec4(self.background_color),
			});

			// Bind the geometry uniforms
			render_pass.set_bind_group(0, &geometry_uniforms.bind_group, &[]);

			render_pass.draw_indexed(0..6, 0, 0..1);
		}
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
	pub fn uniforms(&mut self, wgpu: &Wgpu, shared: &PanelNoneShared) -> &mut PanelNoneGeometryUniforms {
		self.uniforms
			.get_or_insert_with(|| self::create_geometry_uniforms(wgpu, shared))
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

	pub fn geometry_uniforms_bind_group_layout(&self, wgpu: &Wgpu) -> &wgpu::BindGroupLayout {
		self.geometry_uniforms_bind_group_layout
			.get_or_init(|| self::create_geometry_uniforms_bind_group_layout(wgpu))
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


/// Panel none shader
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum PanelNoneShader {
	Basic,
}

impl PanelNoneShader {
	/// Returns this shader's name
	pub fn name(self) -> &'static str {
		match self {
			Self::Basic => "None",
		}
	}

	/// Returns this shader's module as json
	pub fn module_json(self) -> &'static str {
		match self {
			Self::Basic => include_str!(concat!(env!("OUT_DIR"), "/shaders/panels/none.json")),
		}
	}
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
		layout:  shared.geometry_uniforms_bind_group_layout(wgpu),
		entries: &[wgpu::BindGroupEntry {
			binding:  0,
			resource: buffer.as_entire_binding(),
		}],
	};
	let bind_group = wgpu.device.create_bind_group(&bind_group_descriptor);

	PanelNoneGeometryUniforms { buffer, bind_group }
}
