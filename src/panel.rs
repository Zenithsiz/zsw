//! Panel
//!
//! See the [`Panel`] type for more details.

// Modules
mod image;
mod profile;
mod renderer;

// Exports
pub use self::{
	image::PanelImage,
	profile::PanelsProfile,
	renderer::{PanelUniforms, PanelVertex, PanelsRenderer},
};

// Imports
use crate::{
	img::{Image, ImageLoader},
	Rect,
};
use anyhow::Context;
use cgmath::{Matrix4, Vector3};
use std::{
	mem,
	time::{Duration, Instant},
};
use winit::dpi::PhysicalSize;

/// Panel
///
/// A panel is responsible for rendering the scrolling images
/// in a certain rectangle on the window
// Note: It's fine for our state to be public, as
//       we only use it during rendering, and we don't
//       have any expected value for it, nor any cached
//       values that depend on it.
#[derive(Debug)]
pub struct Panel {
	/// Geometry
	pub geometry: Rect<u32>,

	/// Panel state
	pub state: PanelState,

	/// Progress
	pub progress: f32,

	/// Image duration
	pub image_duration: Duration,

	/// Fade point
	// TODO: Ensure it's between 0.5 and 1.0
	pub fade_point: f32,

	/// Next image state
	pub next_image_state: NextImageState,
}

/// Next image
#[derive(Debug)]
pub enum NextImageState {
	/// Ready
	Ready(Image),

	/// Waiting
	Waiting {
		/// Instant we've been waiting since
		since: Instant,
	},

	/// Empty
	Empty,
}

impl NextImageState {
	/// Loads the next image, if waiting or empty
	pub fn load_next(&mut self, image_loader: &ImageLoader) -> Result<(), anyhow::Error> {
		*self = match mem::replace(self, Self::Empty) {
			Self::Ready(image) => Self::Ready(image),
			Self::Waiting { since } => match image_loader.try_recv().context("Unable to receive next image")? {
				Some(image) => {
					log::debug!("Received image after waiting for {:?}", since.elapsed());
					Self::Ready(image)
				},
				None => Self::Waiting { since },
			},
			Self::Empty => match image_loader.try_recv().context("Unable to receive next image")? {
				Some(image) => Self::Ready(image),
				None => Self::Waiting { since: Instant::now() },
			},
		};

		Ok(())
	}

	/// Takes the image, if any
	pub fn take_image(&mut self) -> Option<Image> {
		let image;
		(*self, image) = match mem::replace(self, Self::Empty) {
			Self::Ready(image) => (Self::Empty, Some(image)),
			Self::Waiting { since } => (Self::Waiting { since }, None),
			Self::Empty => (Self::Waiting { since: Instant::now() }, None),
		};
		image
	}
}

impl Panel {
	/// Creates a new panel
	pub const fn new(geometry: Rect<u32>, state: PanelState, image_duration: Duration, fade_point: f32) -> Self {
		Self {
			geometry,
			state,
			progress: 0.0,
			image_duration,
			fade_point,
			next_image_state: NextImageState::Empty,
		}
	}

	/// Updates this panel
	// TODO: Not block if the image isn't ready.
	pub fn update(
		&mut self, device: &wgpu::Device, queue: &wgpu::Queue, uniforms_bind_group_layout: &wgpu::BindGroupLayout,
		texture_bind_group_layout: &wgpu::BindGroupLayout, image_loader: &ImageLoader,
	) -> Result<(), anyhow::Error> {
		// If we've been waiting for an image, try to load it
		self.next_image_state
			.load_next(image_loader)
			.context("Unable to load next image")?;

		// Next frame's progress
		let next_progress = self.progress + (1.0 / 60.0) / self.image_duration.as_secs_f32();

		// Progress on image swap
		let swapped_progress = self.progress - self.fade_point;

		// If we finished the current image
		let finished = self.progress >= 1.0;

		// Update the image state
		// Note: We have to replace the state with `Empty` temporarily due to
		//       panics that might occur while updating.
		(self.state, self.progress) = match mem::replace(&mut self.state, PanelState::Empty) {
			// If we're empty, try to load the next image
			PanelState::Empty => match self.next_image_state.take_image() {
				Some(image) => (
					PanelState::PrimaryOnly {
						front: PanelImage::new(
							device,
							queue,
							uniforms_bind_group_layout,
							texture_bind_group_layout,
							&image,
						)
						.context("Unable to create panel image")?,
					},
					0.0,
				),
				None => (PanelState::Empty, 0.0),
			},

			// If we only have the primary, try to load the next image
			PanelState::PrimaryOnly { front } => match self.next_image_state.take_image() {
				Some(image) => (
					PanelState::Both {
						front,
						back: PanelImage::new(
							device,
							queue,
							uniforms_bind_group_layout,
							texture_bind_group_layout,
							&image,
						)
						.context("Unable to create panel image")?,
					},
					next_progress,
				),
				None => (PanelState::PrimaryOnly { front }, next_progress),
			},

			// If we have both, try to update the progress and swap them if finished
			PanelState::Both { mut front, back } if finished => match self.next_image_state.take_image() {
				// Note: We update the front and swap them
				Some(image) => {
					front
						.update(device, queue, texture_bind_group_layout, &image)
						.context("Unable to update texture")?;
					(
						PanelState::Both {
							front: back,
							back:  front,
						},
						swapped_progress,
					)
				},
				// Note: If we're done without a next image, then just stay at 1.0
				None => (PanelState::Both { front, back }, 1.0),
			},

			// Else just update the progress
			state @ PanelState::Both { .. } => (state, next_progress),
		};

		Ok(())
	}

	/// Draws the panel to `render_pass`
	pub fn draw<'a>(
		&'a mut self, render_pass: &mut wgpu::RenderPass<'a>, queue: &wgpu::Queue, surface_size: PhysicalSize<u32>,
	) {
		// Calculate the matrix for the panel
		let x_scale = self.geometry.size[0] as f32 / surface_size.width as f32;
		let y_scale = self.geometry.size[1] as f32 / surface_size.height as f32;

		let x_offset = self.geometry.pos[0] as f32 / surface_size.width as f32;
		let y_offset = self.geometry.pos[1] as f32 / surface_size.height as f32;

		let matrix = Matrix4::from_translation(Vector3::new(
			-1.0 + x_scale + 2.0 * x_offset,
			1.0 - y_scale - 2.0 * y_offset,
			0.0,
		)) * Matrix4::from_nonuniform_scale(x_scale, -y_scale, 1.0);

		// Calculate the alpha and progress for the back image
		let (back_alpha, back_progress) = match self.progress {
			f if f >= self.fade_point => (
				(self.progress - self.fade_point) / (1.0 - self.fade_point),
				self.progress - self.fade_point,
			),
			_ => (0.0, 0.0),
		};

		// Get the images to render
		let (front, back) = match &mut self.state {
			PanelState::Empty => (None, None),
			PanelState::PrimaryOnly { front, .. } => (Some((front, 1.0, self.progress)), None),
			PanelState::Both { front, back } => (
				Some((front, 1.0 - back_alpha, self.progress)),
				Some((back, back_alpha, back_progress)),
			),
		};

		// Then draw each image
		for (image, alpha, progress) in [front, back].into_iter().flatten() {
			// Skip rendering if alpha is 0
			if alpha == 0.0 {
				continue;
			}

			// Update the uniforms
			let uvs = image.uvs(self.geometry.size);
			let uniforms = PanelUniforms {
				matrix: matrix.into(),
				uvs_start: uvs.start(),
				uvs_offset: uvs.offset(progress),
				alpha,
				_pad: [0.0; 3],
			};
			image.update_uniform(queue, uniforms);

			// Then bind the image and draw it
			image.bind(render_pass);
			render_pass.draw_indexed(0..6, 0, 0..1);
		}
	}
}


/// Panel state
#[derive(Debug)]
pub enum PanelState {
	/// Empty
	///
	/// This means no images have been loaded yet
	Empty,

	/// Primary only
	///
	/// The primary image is loaded. The back image is still not available
	PrimaryOnly {
		/// Image
		front: PanelImage,
	},

	/// Both
	///
	/// Both images are loaded to be faded in between
	Both {
		/// Front image
		front: PanelImage,

		/// Back image
		back: PanelImage,
	},
}
