//! Panel

// Features
#![feature(decl_macro)]

// Modules
mod image;
mod panel;
mod renderer;
mod state;

// Exports
pub use self::{
	image::PanelImage,
	panel::Panel,
	renderer::{PanelUniforms, PanelVertex, PanelsRenderer},
	state::{PanelState, PanelStateImageState, PanelStateImagesState},
};

// Imports
use {
	anyhow::Context,
	std::path::PathBuf,
	zsw_wgpu::{Wgpu, WgpuResizeReceiver, WgpuSurfaceResource},
};


/// Panels editor
#[derive(Clone, Debug)]
#[allow(missing_copy_implementations)] // It might not in the future
pub struct PanelsEditor {
	/// Wgpu
	wgpu: Wgpu,
}

#[allow(clippy::unused_self)] // For accessing resources, we should require the service
impl PanelsEditor {
	/// Adds a new panel
	pub fn add_panel(&mut self, resource: &mut PanelsResource, panel: Panel) {
		resource.panels.push(PanelState::new(resource, &self.wgpu, panel));
	}

	/// Returns all panels
	#[must_use]
	pub fn panels<'a>(&mut self, resource: &'a PanelsResource) -> &'a [PanelState] {
		&resource.panels
	}

	/// Returns all panels, mutably
	#[must_use]
	pub fn panels_mut<'a>(&mut self, resource: &'a mut PanelsResource) -> &'a mut [PanelState] {
		&mut resource.panels
	}

	/// Replaces all panels
	pub fn replace_panels(&mut self, resource: &mut PanelsResource, panels: impl IntoIterator<Item = Panel>) {
		resource.panels = panels
			.into_iter()
			.map(|panel| PanelState::new(resource, &self.wgpu, panel))
			.collect();
	}

	/// Returns the max image size
	#[must_use]
	pub fn max_image_size(&mut self, resource: &PanelsResource) -> Option<u32> {
		resource.max_image_size
	}

	/// Sets the max image size
	pub fn set_max_image_size(&mut self, resource: &mut PanelsResource, max_image_size: Option<u32>) {
		resource.max_image_size = max_image_size;
	}

	/// Returns the max image size mutably
	pub fn max_image_size_mut<'a>(&mut self, resource: &'a mut PanelsResource) -> Option<&'a mut u32> {
		resource.max_image_size.as_mut()
	}

	/// Returns the shader
	#[must_use]
	pub fn shader(&mut self, resource: &PanelsResource) -> PanelsShader {
		resource.shader
	}

	/// Sets the shader
	pub fn set_shader(&mut self, resource: &mut PanelsResource, shader: PanelsShader) {
		resource.shader = shader;
	}

	/// Returns the shader mutably
	pub fn shader_mut<'a>(&mut self, resource: &'a mut PanelsResource) -> &'a mut PanelsShader {
		&mut resource.shader
	}
}

/// Panels resource
#[derive(Debug)]
pub struct PanelsResource {
	/// All panels with their state
	panels: Vec<PanelState>,

	/// Max image size
	max_image_size: Option<u32>,

	/// Shader to use
	shader: PanelsShader,

	/// Uniforms bind group layout
	uniforms_bind_group_layout: wgpu::BindGroupLayout,

	/// Image bind group layout
	image_bind_group_layout: wgpu::BindGroupLayout,
}

/// Shader to render with
#[derive(PartialEq, Clone, Copy, Debug)]
pub enum PanelsShader {
	/// Fade
	Fade,

	/// Fade-white
	FadeWhite { strength: f32 },

	/// Fade-out
	FadeOut { strength: f32 },

	/// Fade-in
	FadeIn { strength: f32 },
}

impl PanelsShader {
	/// Returns the shader name
	#[must_use]
	pub fn name(&self) -> &'static str {
		match self {
			Self::Fade => "Fade",
			Self::FadeWhite { .. } => "Fade White",
			Self::FadeOut { .. } => "Fade Out",
			Self::FadeIn { .. } => "Fade In",
		}
	}
}

/// Creates the panels service
pub fn create(
	wgpu: Wgpu,
	surface_resource: &mut WgpuSurfaceResource,
	wgpu_resize_receiver: WgpuResizeReceiver,
	shader_path: PathBuf,
) -> Result<(PanelsRenderer, PanelsEditor, PanelsResource), anyhow::Error> {
	let uniforms_bind_group_layout = self::create_uniforms_bind_group_layout(&wgpu);
	let image_bind_group_layout = self::create_image_bind_group_layout(&wgpu);

	Ok((
		PanelsRenderer::new(wgpu.clone(), surface_resource, wgpu_resize_receiver, shader_path)
			.context("Unable to create panels renderer")?,
		PanelsEditor { wgpu },
		PanelsResource {
			panels: vec![],
			max_image_size: None,
			shader: PanelsShader::Fade,
			uniforms_bind_group_layout,
			image_bind_group_layout,
		},
	))
}

/// Creates the uniforms bind group layout
fn create_uniforms_bind_group_layout(wgpu: &Wgpu) -> wgpu::BindGroupLayout {
	let descriptor = wgpu::BindGroupLayoutDescriptor {
		label:   Some("[zsw::panel] Uniform bind group layout"),
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

	wgpu.device().create_bind_group_layout(&descriptor)
}

/// Creates the image bind group layout
fn create_image_bind_group_layout(wgpu: &Wgpu) -> wgpu::BindGroupLayout {
	let descriptor = wgpu::BindGroupLayoutDescriptor {
		label:   Some("[zsw::panel] Image bind group layout"),
		entries: &[
			wgpu::BindGroupLayoutEntry {
				binding:    0,
				visibility: wgpu::ShaderStages::FRAGMENT,
				ty:         wgpu::BindingType::Texture {
					multisampled:   false,
					view_dimension: wgpu::TextureViewDimension::D2,
					sample_type:    wgpu::TextureSampleType::Float { filterable: true },
				},
				count:      None,
			},
			wgpu::BindGroupLayoutEntry {
				binding:    1,
				visibility: wgpu::ShaderStages::FRAGMENT,
				ty:         wgpu::BindingType::Texture {
					multisampled:   false,
					view_dimension: wgpu::TextureViewDimension::D2,
					sample_type:    wgpu::TextureSampleType::Float { filterable: true },
				},
				count:      None,
			},
			wgpu::BindGroupLayoutEntry {
				binding:    2,
				visibility: wgpu::ShaderStages::FRAGMENT,
				ty:         wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
				count:      None,
			},
		],
	};

	wgpu.device().create_bind_group_layout(&descriptor)
}
