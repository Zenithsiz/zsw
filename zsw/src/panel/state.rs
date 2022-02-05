//! Panel state

// Imports
use {
	super::PanelImageId,
	crate::{img::ImageReceiver, PanelsRenderer, Rect, Wgpu},
	anyhow::Context,
	cgmath::{Matrix4, Vector3},
	std::time::Duration,
	winit::dpi::PhysicalSize,
};

/// Panel state
#[derive(Debug)]
#[allow(missing_copy_implementations)] // We don't want it to be trivially copyable yet because it manages a resource
pub struct PanelState {
	/// Geometry
	pub geometry: Rect<u32>,

	/// Panel state
	pub state: PanelImageState,

	/// Progress
	pub progress: f32,

	/// Image duration
	pub image_duration: Duration,

	/// Fade point
	// TODO: Ensure it's between 0.5 and 1.0
	pub fade_point: f32,
}

impl PanelState {
	/// Creates a new panel
	#[must_use]
	pub const fn new(geometry: Rect<u32>, state: PanelImageState, image_duration: Duration, fade_point: f32) -> Self {
		Self {
			geometry,
			state,
			progress: 0.0,
			image_duration,
			fade_point,
		}
	}

	/// Updates this panel
	pub fn update(
		&mut self,
		wgpu: &Wgpu,
		renderer: &PanelsRenderer,
		image_rx: &ImageReceiver,
	) -> Result<(), anyhow::Error> {
		// Next frame's progress
		let next_progress = self.progress + (1.0 / 60.0) / self.image_duration.as_secs_f32();

		// Progress on image swap
		let swapped_progress = self.progress - self.fade_point;

		// If we finished the current image
		let finished = self.progress >= 1.0;

		// Update the image state
		(self.state, self.progress) = match self.state {
			// If we're empty, try to get a next image
			PanelImageState::Empty => match image_rx.try_recv().context("Unable to get next image")? {
				Some(image) => (
					PanelImageState::PrimaryOnly {
						front: PanelImageStateImage {
							id:       renderer.create_image(wgpu, image),
							swap_dir: rand::random(),
						},
					},
					// Note: Ensure it's below `0.5` to avoid starting during a fade.
					rand::random::<f32>() / 2.0,
				),
				None => (PanelImageState::Empty, 0.0),
			},

			// If we only have the primary, try to load the next image
			PanelImageState::PrimaryOnly { front } => match image_rx.try_recv().context("Unable to get next image")? {
				Some(image) => (
					PanelImageState::Both {
						front,
						back: PanelImageStateImage {
							id:       renderer.create_image(wgpu, image),
							swap_dir: rand::random(),
						},
					},
					next_progress,
				),
				None => (PanelImageState::PrimaryOnly { front }, next_progress),
			},

			// If we have both, try to update the progress and swap them if finished
			PanelImageState::Both { mut front, back } if finished => {
				match image_rx.try_recv().context("Unable to get next image")? {
					// Note: We update the front and swap them
					Some(image) => {
						renderer.update_image(wgpu, front.id, image);
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

	/// Calculates the matrix for this panel
	// Note: This matrix simply goes from a geometry in physical units
	//       onto shader coordinates.
	#[must_use]
	pub fn matrix(&self, surface_size: PhysicalSize<u32>) -> Matrix4<f32> {
		let x_scale = self.geometry.size[0] as f32 / surface_size.width as f32;
		let y_scale = self.geometry.size[1] as f32 / surface_size.height as f32;

		let x_offset = self.geometry.pos[0] as f32 / surface_size.width as f32;
		let y_offset = self.geometry.pos[1] as f32 / surface_size.height as f32;

		let translation = Matrix4::from_translation(Vector3::new(
			-1.0 + x_scale + 2.0 * x_offset,
			1.0 - y_scale - 2.0 * y_offset,
			0.0,
		));
		let scaling = Matrix4::from_nonuniform_scale(x_scale, -y_scale, 1.0);
		translation * scaling
	}

	/// Returns all images descriptors to render
	#[must_use]
	pub fn image_descriptors(&self) -> impl IntoIterator<Item = PanelImageDescriptor> + '_ {
		// Calculate the alpha and progress for the back image
		let (back_alpha, back_progress) = match self.progress {
			f if f >= self.fade_point => (
				(self.progress - self.fade_point) / (1.0 - self.fade_point),
				self.progress - self.fade_point,
			),
			_ => (0.0, 0.0),
		};

		// Get the images to render
		let (front, back) = match self.state {
			PanelImageState::Empty => (None, None),
			PanelImageState::PrimaryOnly { front, .. } => (
				Some(PanelImageDescriptor {
					image_id: front.id,
					alpha:    1.0,
					progress: self.progress,
					swap_dir: front.swap_dir,
				}),
				None,
			),
			PanelImageState::Both { front, back } => (
				Some(PanelImageDescriptor {
					image_id: front.id,
					alpha:    1.0 - back_alpha,
					progress: self.progress,
					swap_dir: front.swap_dir,
				}),
				Some(PanelImageDescriptor {
					image_id: back.id,
					alpha:    back_alpha,
					progress: back_progress,
					swap_dir: back.swap_dir,
				}),
			),
		};

		[front, back].into_iter().flatten()
	}
}

/// Panel image descriptor
///
/// Used to describe the state of each image to be
/// drawn for a panel.
#[derive(Clone, Copy, Debug)]
pub struct PanelImageDescriptor {
	/// Image
	pub image_id: PanelImageId,

	/// Alpha
	pub alpha: f32,

	/// Progress
	pub progress: f32,

	/// Swap direction?
	pub swap_dir: bool,
}


/// Panel image state
#[derive(Clone, Copy, Debug)]
pub enum PanelImageState {
	/// Empty
	///
	/// This means no images have been loaded yet
	Empty,

	/// Primary only
	///
	/// The primary image is loaded. The back image is still not available
	PrimaryOnly {
		/// Image
		front: PanelImageStateImage,
	},

	/// Both
	///
	/// Both images are loaded to be faded in between
	Both {
		/// Front image
		front: PanelImageStateImage,

		/// Back image
		back: PanelImageStateImage,
	},
}

/// Panel image state image
#[derive(Clone, Copy, Debug)]
pub struct PanelImageStateImage {
	/// Image id
	id: PanelImageId,

	/// If swapping directions
	swap_dir: bool,
}
