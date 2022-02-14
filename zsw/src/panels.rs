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
	crate::{
		util::{extse::ParkingLotMutexSe, MightBlock},
		ImageLoader,
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

	/// All panels with their state
	// TODO: Make `async`
	panels: Mutex<Vec<PanelState>>,
}

impl Panels {
	/// Creates the panel
	pub fn new(device: &wgpu::Device, surface_texture_format: wgpu::TextureFormat) -> Result<Self, anyhow::Error> {
		// Create the renderer
		let renderer = PanelsRenderer::new(device, surface_texture_format).context("Unable to create renderer")?;

		Ok(Self {
			renderer,
			panels: Mutex::new(vec![]),
		})
	}

	/// Adds a new panel
	pub fn add_panel(&self, panel: Panel) {
		// DEADLOCK: We ensure this lock can't deadlock by not blocking
		//           while locked.
		self.panels.lock_se().allow::<MightBlock>().push(PanelState::new(panel));
	}

	/// Returns all panels
	pub fn panels(&self) -> Vec<Panel> {
		// DEADLOCK: We ensure this lock can't deadlock by not blocking
		//           while locked.
		self.panels
			.lock_se()
			.allow::<MightBlock>()
			.iter()
			.map(|panel| panel.panel)
			.collect()
	}

	/// Replaces all panels
	pub fn replace_panels(&self, new_panels: impl IntoIterator<Item = Panel>) {
		// DEADLOCK: We ensure this lock can't deadlock by not blocking
		//           while locked.
		let mut panels = self.panels.lock_se().allow::<MightBlock>();

		*panels = new_panels.into_iter().map(PanelState::new).collect();
	}

	/// Updates all panels
	pub fn update_all(&self, wgpu: &Wgpu, image_loader: &ImageLoader) -> Result<(), anyhow::Error> {
		// DEADLOCK: We ensure this lock can't deadlock by not blocking
		//           while locked.
		let mut panels = self.panels.lock_se().allow::<MightBlock>();

		for panel in &mut *panels {
			panel
				.update(&self.renderer, wgpu, image_loader)
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
			.render(panels.iter(), queue, encoder, surface_view, surface_size)
	}
}