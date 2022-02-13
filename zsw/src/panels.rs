//! Panel

// Modules
mod image;
mod panel;
mod profile;
mod renderer;
mod state;

// Exports
pub use self::{
	image::PanelImage,
	panel::Panel,
	profile::PanelsProfile,
	renderer::{PanelImageId, PanelUniforms, PanelVertex, PanelsRenderer},
	state::{PanelImageStateImage, PanelState, PanelStateImages},
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
	std::mem,
	winit::dpi::PhysicalSize,
	zsw_side_effect_macros::side_effect,
};


/// Panels
#[derive(Debug)]
pub struct Panels {
	/// Panels renderer
	renderer: PanelsRenderer,

	/// All panels with their state
	panels: Mutex<Vec<PanelState>>,
}

impl Panels {
	/// Creates the panel
	pub fn new(
		panels: impl IntoIterator<Item = Panel>,
		device: &wgpu::Device,
		surface_texture_format: wgpu::TextureFormat,
	) -> Result<Self, anyhow::Error> {
		// Create the renderer
		let renderer = PanelsRenderer::new(device, surface_texture_format).context("Unable to create renderer")?;

		// Collect all panels
		let panels = panels.into_iter().map(PanelState::new).collect();

		Ok(Self {
			renderer,
			panels: Mutex::new(panels),
		})
	}

	/// Adds a new panel
	pub fn add_panel(&self, panel: Panel) {
		// DEADLOCK: We ensure this lock can't deadlock by not blocking
		//           while locked.
		self.panels.lock_se().allow::<MightBlock>().push(PanelState::new(panel));
	}

	/// Updates all panels
	pub fn update_all(&self, wgpu: &Wgpu, image_loader: &ImageLoader) -> Result<(), anyhow::Error> {
		// DEADLOCK: We ensure this lock can't deadlock by not blocking
		//           while locked.
		let mut panels = self.panels.lock_se().allow::<MightBlock>();

		for panel in &mut *panels {
			self.update_panel(panel, wgpu, image_loader)
				.context("Unable to update panel")?;
		}

		Ok(())
	}

	/// Updates a panel
	fn update_panel(
		&self,
		panel: &mut PanelState,
		wgpu: &Wgpu,
		image_loader: &ImageLoader,
	) -> Result<(), anyhow::Error> {
		// Next frame's progress
		let next_progress = panel.cur_progress.saturating_add(1).clamp(0, panel.panel.duration);

		// Progress on image swap
		let swapped_progress = panel.cur_progress.saturating_sub(panel.panel.fade_point);

		// If we finished the current image
		let finished = panel.cur_progress >= panel.panel.duration;

		// Update the image state
		// Note: We're only `take`ing the images because we need them by value
		(panel.images, panel.cur_progress) = match mem::take(&mut panel.images) {
			// If we're empty, try to get a next image
			PanelStateImages::Empty => match image_loader.try_recv() {
				#[allow(clippy::cast_sign_loss)] // It's positive
				Some(image) => (
					PanelStateImages::PrimaryOnly {
						front: PanelImageStateImage {
							id:       self.renderer.create_image(wgpu, image),
							swap_dir: rand::random(),
						},
					},
					// Note: Ensure it's below `0.5` to avoid starting during a fade.
					(rand::random::<f32>() / 2.0 * panel.panel.duration as f32) as u64,
				),
				None => (PanelStateImages::Empty, 0),
			},

			// If we only have the primary, try to load the next image
			PanelStateImages::PrimaryOnly { front } => match image_loader.try_recv() {
				Some(image) => (
					PanelStateImages::Both {
						front,
						back: PanelImageStateImage {
							id:       self.renderer.create_image(wgpu, image),
							swap_dir: rand::random(),
						},
					},
					next_progress,
				),
				None => (PanelStateImages::PrimaryOnly { front }, next_progress),
			},

			// If we have both, try to update the progress and swap them if finished
			PanelStateImages::Both { mut front, back } if finished => {
				match image_loader.try_recv() {
					// Note: We update the front and swap them
					Some(image) => {
						self.renderer.update_image(wgpu, front.id, image);
						front.swap_dir = rand::random();
						(
							PanelStateImages::Both {
								front: back,
								back:  front,
							},
							swapped_progress,
						)
					},
					None => (PanelStateImages::Both { front, back }, next_progress),
				}
			},

			// Else just update the progress
			state @ PanelStateImages::Both { .. } => (state, next_progress),
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
			.render(panels.iter(), queue, encoder, surface_view, surface_size)
	}
}
