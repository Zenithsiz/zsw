//! Panel
//!
//! See the [`Panel`] type for more details.

// Modules
mod image;
mod renderer;

// Exports
pub use self::{
	image::PanelImage,
	renderer::{PanelUniforms, PanelVertex, PanelsRenderer},
};

// Imports
use crate::{img::ImageLoader, Rect};
use anyhow::Context;
use cgmath::{Matrix4, Vector3};
use std::time::{Duration, Instant};
use winit::dpi::PhysicalSize;

/// Panel
///
/// A panel is responsible for rendering the scrolling images
/// in a certain rectangle on the window
#[derive(Debug)]
pub struct Panel {
	/// Geometry
	geometry: Rect<u32>,

	/// Panel state
	state: PanelState,

	/// Progress
	progress: f32,

	/// Image duration
	image_duration: Duration,

	/// Fade point
	fade_point: f32,

	/// Image backlog
	image_backlog: usize,
}

impl Panel {
	/// Creates a new panel
	pub const fn new(
		geometry: Rect<u32>, state: PanelState, image_duration: Duration, fade_point: f32, image_backlog: usize,
	) -> Self {
		Self {
			geometry,
			state,
			progress: 0.0,
			image_duration,
			fade_point,
			image_backlog,
		}
	}

	/// Updates this panel
	pub fn update(
		&mut self, device: &wgpu::Device, queue: &wgpu::Queue, uniforms_bind_group_layout: &wgpu::BindGroupLayout,
		texture_bind_group_layout: &wgpu::BindGroupLayout, image_loader: &ImageLoader,
	) -> Result<(), anyhow::Error> {
		// Next frame's progress
		let next_progress = self.progress + (1.0 / 60.0) / self.image_duration.as_secs_f32();

		// Progress on image swap
		let swapped_progress = self.progress - self.fade_point;

		// If past the fade point
		let past_fade = self.progress >= self.fade_point;

		// If we finished the current image
		let finished = self.progress >= 1.0;

		// Check the image state
		let geometry = self.geometry;
		(self.state, self.progress) = match std::mem::replace(&mut self.state, PanelState::Empty) {
			// If we're empty, get the next image
			PanelState::Empty => {
				let image = PanelImage::new(
					device,
					queue,
					uniforms_bind_group_layout,
					texture_bind_group_layout,
					image_loader,
					geometry.size,
					self.image_backlog,
				)
				.context("Unable to create image")?;
				(PanelState::PrimaryOnly { image }, 0.0)
			},

			// If we only have the primary and we're past the fade point, get the next image
			// Note: We do this so we can render the first image without waiting
			//       for both images to load
			// TODO: Redo this setup
			PanelState::PrimaryOnly { image: front } if past_fade => {
				let back = PanelImage::new(
					device,
					queue,
					uniforms_bind_group_layout,
					texture_bind_group_layout,
					image_loader,
					geometry.size,
					self.image_backlog,
				)
				.context("Unable to create image")?;

				(PanelState::Both { front, back }, next_progress)
			},

			// If we have both, update the progress and swap them if finished
			PanelState::Both { front, back } if finished => {
				// Note: Front and back are swapped here since we implicitly swap
				match self::update_swapped(
					front,
					back,
					None,
					device,
					queue,
					texture_bind_group_layout,
					image_loader,
					past_fade,
				)
				.context("Unable to update swapped image")?
				{
					(true, state) => (state, swapped_progress),
					(false, state) => (state, next_progress),
				}
			},

			// If we're swapped, try to update
			PanelState::Swapped { front, back, since } => match self::update_swapped(
				back,
				front,
				Some(since),
				device,
				queue,
				texture_bind_group_layout,
				image_loader,
				past_fade,
			)
			.context("Unable to update swapped image")?
			{
				(true, state) => (state, swapped_progress),
				(false, state) => (state, next_progress),
			},

			// Else keep the current state and advance
			state => (state, next_progress),
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
			PanelState::PrimaryOnly { image, .. } | PanelState::Swapped { front: image, .. } => {
				(Some((image, 1.0, self.progress)), None)
			},
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
			let uvs = image.uvs();
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
		image: PanelImage,
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

	/// Swapped
	///
	/// Front and back images have been swapped, and the next image needs
	/// to be loaded
	Swapped {
		/// Front image
		front: PanelImage,

		/// Back image that needs to be swapped
		back: PanelImage,

		/// Instant we were swapped
		since: Instant,
	},
}

/// Updates a swapped image state and returns the next state
#[allow(clippy::too_many_arguments)] // TODO:
fn update_swapped(
	mut back: PanelImage, front: PanelImage, mut since: Option<Instant>, device: &wgpu::Device, queue: &wgpu::Queue,
	texture_bind_group_layout: &wgpu::BindGroupLayout, image_loader: &ImageLoader, force_wait: bool,
) -> Result<(bool, PanelState), anyhow::Error> {
	// If we're force waiting and don't have a `since`, create it,
	// so we can keep track of how long the request took
	if force_wait && since.is_none() {
		since = Some(Instant::now());
	}

	let swapped = back
		.try_update(device, queue, texture_bind_group_layout, image_loader, force_wait)
		.context("Unable to get next image")?;
	let state = match swapped {
		// If we updated, switch to `Both`
		true => {
			// If we didn't just update it, log how long it took
			if let Some(since) = since {
				let duration = Instant::now().saturating_duration_since(since);
				log::trace!("Waited {duration:?} for the next image");
			}
			PanelState::Both { front, back }
		},

		// Else stay in `Swapped`
		false => PanelState::Swapped {
			back,
			front,
			since: since.unwrap_or_else(Instant::now),
		},
	};

	Ok((swapped, state))
}
