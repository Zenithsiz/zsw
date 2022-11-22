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

// Imports
use {
	anyhow::Context,
	cgmath::Point2,
	winit::dpi::PhysicalSize,
	zsw_img::ImageReceiver,
	zsw_input::Input,
	zsw_wgpu::Wgpu,
};


/// Panels service
// TODO: Rename to `PanelsService`
#[derive(Debug)]
pub struct Panels {
	/// Panels renderer
	renderer: PanelsRenderer,
}

#[allow(clippy::unused_self)] // For accessing resources, we should require the service
impl Panels {
	/// Creates the panel, alongside it's resources
	pub fn new(
		device: &wgpu::Device,
		surface_texture_format: wgpu::TextureFormat,
	) -> Result<(Self, PanelsResource), anyhow::Error> {
		// Create the renderer
		let renderer = PanelsRenderer::new(device, surface_texture_format).context("Unable to create renderer")?;

		// Create the service
		let service = Self { renderer };

		// Create our resource
		let resource = PanelsResource { panels: vec![] };

		Ok((service, resource))
	}

	/// Adds a new panel
	pub fn add_panel(&self, resource: &mut PanelsResource, panel: Panel) {
		resource.panels.push(PanelState::new(panel));
	}

	/// Returns all panels
	pub fn panels<'a>(&self, resource: &'a PanelsResource) -> &'a [PanelState] {
		&resource.panels
	}

	/// Returns all panels, mutably
	pub fn panels_mut<'a>(&self, resource: &'a mut PanelsResource) -> &'a mut [PanelState] {
		&mut resource.panels
	}

	/// Replaces all panels
	pub fn replace_panels(&self, resource: &mut PanelsResource, panels: impl IntoIterator<Item = Panel>) {
		resource.panels = panels.into_iter().map(PanelState::new).collect();
	}

	/// Updates all panels
	pub fn update_all(
		&self,
		resource: &mut PanelsResource,
		wgpu: &Wgpu,
		image_receiver: &ImageReceiver,
	) -> Result<(), anyhow::Error> {
		for panel in &mut resource.panels {
			panel
				.update(&self.renderer, wgpu, image_receiver)
				.context("Unable to update panel")?;
		}

		Ok(())
	}

	/// Renders all panels
	pub fn render(
		&self,
		input: &Input,
		resource: &PanelsResource,
		queue: &wgpu::Queue,
		encoder: &mut wgpu::CommandEncoder,
		surface_view: &wgpu::TextureView,
		surface_size: PhysicalSize<u32>,
	) -> Result<(), anyhow::Error> {
		let cursor_pos = input
			.cursor_pos()
			.map_or(Point2::new(0, 0), |pos| Point2::new(pos.x as i32, pos.y as i32));

		// Then render
		self.renderer
			.render(&resource.panels, cursor_pos, queue, encoder, surface_view, surface_size)
	}
}

/// Panels resource
#[derive(Debug)]
pub struct PanelsResource {
	/// All panels with their state
	panels: Vec<PanelState>,
}
