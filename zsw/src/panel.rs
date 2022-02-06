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
			self.update_panel(panel, wgpu).context("Unable to update panel")?;
		}

		Ok(())
	}

	/// Updates a panel
	pub fn update_panel(&self, panel: &mut PanelState, wgpu: &Wgpu) -> Result<(), anyhow::Error> {
		// Next frame's progress
		let next_progress = panel.progress + (1.0 / 60.0) / panel.image_duration.as_secs_f32();

		// Progress on image swap
		let swapped_progress = panel.progress - panel.fade_point;

		// If we finished the current image
		let finished = panel.progress >= 1.0;

		// Update the image state
		(panel.state, panel.progress) = match panel.state {
			// If we're empty, try to get a next image
			PanelImageState::Empty => match self.image_rx.try_recv().context("Unable to get next image")? {
				Some(image) => (
					PanelImageState::PrimaryOnly {
						front: PanelImageStateImage {
							id:       self.renderer.create_image(wgpu, image),
							swap_dir: rand::random(),
						},
					},
					// Note: Ensure it's below `0.5` to avoid starting during a fade.
					rand::random::<f32>() / 2.0,
				),
				None => (PanelImageState::Empty, 0.0),
			},

			// If we only have the primary, try to load the next image
			PanelImageState::PrimaryOnly { front } =>
				match self.image_rx.try_recv().context("Unable to get next image")? {
					Some(image) => (
						PanelImageState::Both {
							front,
							back: PanelImageStateImage {
								id:       self.renderer.create_image(wgpu, image),
								swap_dir: rand::random(),
							},
						},
						next_progress,
					),
					None => (PanelImageState::PrimaryOnly { front }, next_progress),
				},

			// If we have both, try to update the progress and swap them if finished
			PanelImageState::Both { mut front, back } if finished => {
				match self.image_rx.try_recv().context("Unable to get next image")? {
					// Note: We update the front and swap them
					Some(image) => {
						self.renderer.update_image(wgpu, front.id, image);
						front.swap_dir = rand::random();
						(
							PanelImageState::Both {
								front: back,
								back:  front,
							},
							swapped_progress,
						)
					},
					// Note: If we're done without a next image, then just stay at 1.0
					None => (PanelImageState::Both { front, back }, 1.0),
				}
			},

			// Else just update the progress
			state @ PanelImageState::Both { .. } => (state, next_progress),
		};

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
