//! Panel

// Modules
mod image;
mod profile;
mod renderer;
mod state;

// Exports
pub use self::{
	image::PanelImage,
	profile::PanelsProfile,
	renderer::{PanelImageId, PanelUniforms, PanelVertex, PanelsRenderer},
	state::{PanelImageDescriptor, PanelImageState, PanelImageStateImage, PanelState},
};

// Imports
use {
	crate::{
		img::ImageReceiver,
		util::{extse::ParkingLotMutexSe, MightBlock},
		Wgpu,
	},
	anyhow::Context,
	parking_lot::Mutex,
	winit::dpi::PhysicalSize,
	zsw_side_effect_macros::side_effect,
};


/// Panels
#[derive(Debug)]
pub struct Panels {
	/// Panels renderer
	renderer: PanelsRenderer,

	/// Image receiver
	image_rx: ImageReceiver,

	/// All of the panels
	panels: Mutex<Vec<PanelState>>,
}

impl Panels {
	/// Creates the panel
	pub fn new(
		panels: impl IntoIterator<Item = PanelState>,
		image_rx: ImageReceiver,
		device: &wgpu::Device,
		surface_texture_format: wgpu::TextureFormat,
	) -> Result<Self, anyhow::Error> {
		// Create the renderer
		let renderer = PanelsRenderer::new(device, surface_texture_format).context("Unable to create renderer")?;

		Ok(Self {
			renderer,
			image_rx,
			panels: Mutex::new(panels.into_iter().collect()),
		})
	}

	/// Adds a new panel
	pub fn add_panel(&self, panel: PanelState) {
		// DEADLOCK: We ensure this lock can't deadlock by not blocking
		//           while locked.
		self.panels.lock_se().allow::<MightBlock>().push(panel);
	}

	/// Updates all panels
	pub fn update_all(&self, wgpu: &Wgpu) -> Result<(), anyhow::Error> {
		// DEADLOCK: We ensure this lock can't deadlock by not blocking
		//           while locked.
		let mut panels = self.panels.lock_se().allow::<MightBlock>();

		for panel in &mut *panels {
			panel
				.update(wgpu, &self.renderer, &self.image_rx)
				.context("Unable to update panel")?;
		}

		Ok(())
	}

	/// Iterates over all panels mutably.
	///
	/// # Blocking
	/// Will deadlock if `f` blocks.
	#[side_effect(MightBlock)]
	pub fn for_each_mut<T, C: FromIterator<T>>(&self, f: impl FnMut(&mut PanelState) -> T) -> C {
		// DEADLOCK: We ensure this lock can't deadlock by not blocking
		//           while locked.
		//           Caller ensures `f` won't block
		let mut panels = self.panels.lock_se().allow::<MightBlock>();
		panels.iter_mut().map(f).collect()
	}

	/// Renders all panels
	pub fn render(
		&self,
		queue: &wgpu::Queue,
		encoder: &mut wgpu::CommandEncoder,
		surface_view: &wgpu::TextureView,
		surface_size: PhysicalSize<u32>,
	) -> Result<(), anyhow::Error> {
		// DEADLOCK: We ensure this lock can't deadlock by not blocking
		//           while locked. `PanelRenderer::render` doesn't block.
		let panels = self.panels.lock_se().allow::<MightBlock>();

		// Then render
		self.renderer
			.render(&*panels, queue, encoder, surface_view, surface_size)
	}
}
