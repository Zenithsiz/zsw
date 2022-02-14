//! Panel state

// Imports
use {
	super::PanelImage,
	crate::{Panel, PanelsRenderer},
	cgmath::{Matrix4, Vector3},
	std::mem,
	winit::dpi::PhysicalSize,
	zsw_img::ImageLoader,
	zsw_wgpu::Wgpu,
};

/// Panel state
#[derive(Debug)]
pub struct PanelState {
	/// Panel
	pub panel: Panel,

	/// Images
	pub images: PanelStateImages,

	/// Current progress (in frames)
	pub cur_progress: u64,
}

impl PanelState {
	/// Creates a new panel
	#[must_use]
	pub const fn new(panel: Panel) -> Self {
		Self {
			panel,
			images: PanelStateImages::Empty,
			cur_progress: 0,
		}
	}

	/// Updates this panel's state
	pub fn update(
		&mut self,
		renderer: &PanelsRenderer,
		wgpu: &Wgpu,
		image_loader: &ImageLoader,
	) -> Result<(), anyhow::Error> {
		// Next frame's progress
		let next_progress = self.cur_progress.saturating_add(1).clamp(0, self.panel.duration);

		// Progress on image swap
		let swapped_progress = self.cur_progress.saturating_sub(self.panel.fade_point);

		// If we finished the current image
		let finished = self.cur_progress >= self.panel.duration;

		// Update the image state
		// Note: We're only `take`ing the images because we need them by value
		(self.images, self.cur_progress) = match mem::take(&mut self.images) {
			// If we're empty, try to get a next image
			PanelStateImages::Empty => match image_loader.try_recv() {
				#[allow(clippy::cast_sign_loss)] // It's positive
				Some(image) => (
					PanelStateImages::PrimaryOnly {
						front: PanelStateImage {
							image:    PanelImage::new(renderer, wgpu, image),
							swap_dir: rand::random(),
						},
					},
					// Note: Ensure it's below `0.5` to avoid starting during a fade.
					(rand::random::<f32>() / 2.0 * self.panel.duration as f32) as u64,
				),
				None => (PanelStateImages::Empty, 0),
			},

			// If we only have the primary, try to load the next image
			PanelStateImages::PrimaryOnly { front } => match image_loader.try_recv() {
				Some(image) => (
					PanelStateImages::Both {
						front,
						back: PanelStateImage {
							image:    PanelImage::new(renderer, wgpu, image),
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
						front.image.update(renderer, wgpu, image);
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

	/// Calculates this panel's matrix
	// Note: This matrix simply goes from a geometry in physical units
	//       onto shader coordinates.
	#[must_use]
	pub fn matrix(&self, surface_size: PhysicalSize<u32>) -> Matrix4<f32> {
		let x_scale = self.panel.geometry.size[0] as f32 / surface_size.width as f32;
		let y_scale = self.panel.geometry.size[1] as f32 / surface_size.height as f32;

		let x_offset = self.panel.geometry.pos[0] as f32 / surface_size.width as f32;
		let y_offset = self.panel.geometry.pos[1] as f32 / surface_size.height as f32;

		let translation = Matrix4::from_translation(Vector3::new(
			-1.0 + x_scale + 2.0 * x_offset,
			1.0 - y_scale - 2.0 * y_offset,
			0.0,
		));
		let scaling = Matrix4::from_nonuniform_scale(x_scale, -y_scale, 1.0);
		translation * scaling
	}

	/// Returns all image descriptors to render
	#[must_use]
	pub fn image_descriptors(&self) -> impl IntoIterator<Item = PanelStateImageDescriptor> + '_ {
		// Calculate the alpha and progress for the back image
		let (back_alpha, back_progress) = match self.cur_progress {
			f if f >= self.panel.fade_point => (
				(self.cur_progress - self.panel.fade_point) as f32 /
					(self.panel.duration - self.panel.fade_point) as f32,
				(self.cur_progress - self.panel.fade_point) as f32 / self.panel.duration as f32,
			),
			_ => (0.0, 0.0),
		};

		// Progress, clamped to `0.0..1.0`
		let progress = self.cur_progress as f32 / self.panel.duration as f32;

		// Get the images to render
		let (front, back) = match &self.images {
			PanelStateImages::Empty => (None, None),
			PanelStateImages::PrimaryOnly { front, .. } => (
				Some(PanelStateImageDescriptor {
					image: &front.image,
					alpha: 1.0,
					progress,
					swap_dir: front.swap_dir,
				}),
				None,
			),
			PanelStateImages::Both { front, back } => (
				Some(PanelStateImageDescriptor {
					image: &front.image,
					alpha: 1.0 - back_alpha,
					progress,
					swap_dir: front.swap_dir,
				}),
				Some(PanelStateImageDescriptor {
					image:    &back.image,
					alpha:    back_alpha,
					progress: back_progress,
					swap_dir: back.swap_dir,
				}),
			),
		};

		[front, back].into_iter().flatten()
	}
}

/// Images for a panel state
#[derive(Default, Debug)]
#[allow(clippy::large_enum_variant)] // They'll all progress towards the largest variant
pub enum PanelStateImages {
	/// Empty
	///
	/// This means no images have been loaded yet
	#[default]
	Empty,

	/// Primary only
	///
	/// The primary image is loaded. The back image is still not available
	PrimaryOnly {
		/// Image
		front: PanelStateImage,
	},

	/// Both
	///
	/// Both images are loaded to be faded in between
	Both {
		/// Front image
		front: PanelStateImage,

		/// Back image
		back: PanelStateImage,
	},
}

/// Single image of a panel state
#[derive(Debug)]
pub struct PanelStateImage {
	/// Image
	pub image: PanelImage,

	/// If swapping directions for this image
	pub swap_dir: bool,
}

/// Panel image descriptor
#[derive(Clone, Copy, Debug)]
pub struct PanelStateImageDescriptor<'a> {
	/// Image
	pub image: &'a PanelImage,

	/// Alpha
	pub alpha: f32,

	/// Progress
	pub progress: f32,

	/// Swap direction?
	pub swap_dir: bool,
}
