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
	state::{PanelState, PanelStateImage, PanelStateImages},
};

// Imports
use zsw_wgpu::{Wgpu, WgpuResizeReceiver, WgpuSurfaceResource};


/// Panels editor
#[derive(Clone, Debug)]
#[allow(missing_copy_implementations)] // It might not in the future
pub struct PanelsEditor {}

#[allow(clippy::unused_self)] // For accessing resources, we should require the service
impl PanelsEditor {
	/// Adds a new panel
	pub fn add_panel(&mut self, resource: &mut PanelsResource, panel: Panel) {
		resource.panels.push(PanelState::new(panel));
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
		resource.panels = panels.into_iter().map(PanelState::new).collect();
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
#[must_use]
pub fn create(
	wgpu: Wgpu,
	surface_resource: &mut WgpuSurfaceResource,
	wgpu_resize_receiver: WgpuResizeReceiver,
) -> (PanelsRenderer, PanelsEditor, PanelsResource) {
	(
		PanelsRenderer::new(wgpu, surface_resource, wgpu_resize_receiver),
		PanelsEditor {},
		PanelsResource {
			panels:         vec![],
			max_image_size: None,
			shader:         PanelsShader::Fade,
		},
	)
}
