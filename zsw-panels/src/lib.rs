//! Panel

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

/// Panels editor
#[derive(Debug)]
#[allow(missing_copy_implementations)] // It might not in the future
pub struct PanelsEditor {}

#[allow(clippy::unused_self)] // For accessing resources, we should require the service
impl PanelsEditor {
	/// Adds a new panel
	pub fn add_panel(&self, resource: &mut PanelsResource, panel: Panel) {
		resource.panels.push(PanelState::new(panel));
	}

	/// Returns all panels
	#[must_use]
	pub fn panels<'a>(&self, resource: &'a PanelsResource) -> &'a [PanelState] {
		&resource.panels
	}

	/// Returns all panels, mutably
	#[must_use]
	pub fn panels_mut<'a>(&self, resource: &'a mut PanelsResource) -> &'a mut [PanelState] {
		&mut resource.panels
	}

	/// Replaces all panels
	pub fn replace_panels(&self, resource: &mut PanelsResource, panels: impl IntoIterator<Item = Panel>) {
		resource.panels = panels.into_iter().map(PanelState::new).collect();
	}
}

/// Panels resource
#[derive(Debug)]
pub struct PanelsResource {
	/// All panels with their state
	panels: Vec<PanelState>,
}

/// Creates the panels service
#[must_use]
pub fn create(
	device: &wgpu::Device,
	surface_texture_format: wgpu::TextureFormat,
) -> (PanelsRenderer, PanelsEditor, PanelsResource) {
	(
		PanelsRenderer::new(device, surface_texture_format),
		PanelsEditor {},
		PanelsResource { panels: vec![] },
	)
}
