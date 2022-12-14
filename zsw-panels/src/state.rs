//! Panel state

// Imports
use {
	super::PanelImage,
	crate::{Panel, PanelsResource},
	cgmath::{Matrix4, Point2, Vector2, Vector3},
	num_rational::Rational32,
	std::mem,
	wgpu::util::DeviceExt,
	winit::dpi::PhysicalSize,
	zsw_img::{ImageReceiver, RawImageProvider},
	zsw_wgpu::Wgpu,
};

/// Panel state
#[derive(Debug)]
pub struct PanelState {
	/// Panel
	pub panel: Panel,

	/// Images
	pub images: PanelStateImagesState,

	/// Front image
	pub front_image: PanelImage,

	/// Back image
	pub back_image: PanelImage,

	/// Texture sampler
	pub texture_sampler: wgpu::Sampler,

	/// Texture bind group
	pub image_bind_group: wgpu::BindGroup,

	/// Uniforms
	pub uniforms: wgpu::Buffer,

	/// Uniforms bind group
	pub uniforms_bind_group: wgpu::BindGroup,

	/// Current progress (in frames)
	pub cur_progress: u64,
}

impl PanelState {
	/// Creates a new panel
	#[must_use]
	pub fn new(resource: &PanelsResource, wgpu: &Wgpu, panel: Panel) -> Self {
		// Create the uniforms
		// Note: Initial value doesn't matter
		let uniforms_descriptor = wgpu::util::BufferInitDescriptor {
			label:    None,
			// TODO: Resize buffer as we go?
			contents: &[0; 0x100],
			usage:    wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
		};
		let uniforms = wgpu.device().create_buffer_init(&uniforms_descriptor);

		// Create the uniform bind group
		let uniforms_bind_group_descriptor = wgpu::BindGroupDescriptor {
			layout:  &resource.uniforms_bind_group_layout,
			entries: &[wgpu::BindGroupEntry {
				binding:  0,
				resource: uniforms.as_entire_binding(),
			}],
			label:   None,
		};
		let uniforms_bind_group = wgpu.device().create_bind_group(&uniforms_bind_group_descriptor);

		// Create the textures
		let front_image = PanelImage::new(wgpu);
		let back_image = PanelImage::new(wgpu);
		let texture_sampler = self::create_texture_sampler(wgpu.device());
		let image_bind_group = self::create_image_bind_group(
			wgpu,
			&resource.image_bind_group_layout,
			&front_image.texture_view,
			&back_image.texture_view,
			&texture_sampler,
		);

		Self {
			panel,
			images: PanelStateImagesState::Empty,
			front_image,
			back_image,
			texture_sampler,
			image_bind_group,
			uniforms,
			uniforms_bind_group,
			cur_progress: 0,
		}
	}

	/// Updates this panel's state
	#[allow(clippy::too_many_lines)] // TODO: Refactor
	pub fn update<P: RawImageProvider>(
		&mut self,
		wgpu: &Wgpu,
		image_receiver: &ImageReceiver<P>,
		image_bind_group_layout: &wgpu::BindGroupLayout,
		max_image_size: Option<u32>,
	) {
		// Next frame's progress
		let next_progress = self.cur_progress.saturating_add(1).clamp(0, self.panel.duration);

		// Progress on image swap
		let swapped_progress = self.cur_progress.saturating_sub(self.panel.fade_point);

		// If we finished the current image
		let finished = self.cur_progress >= self.panel.duration;

		// Update the image state
		// Note: We're only `take`ing the images because we need them by value
		(self.images, self.cur_progress) = 'update: loop {
			// Handles a `Result<T, ImageTooBigError>`
			macro handle_image_too_big_error($image:expr, $res:expr, $default_panels_images:expr) {
				match $res {
					Ok(value) => value,
					Err(err) => match $image {
						image => {
							// Try to resize
							// Note: If we can't resize, we just instead remove
							tracing::warn!("Unable to use image {}: {err}", image.name);
							if let Err(image) = image_receiver.queue_resize(image, err.max_image_size) {
								// Note: If we can't remove, just drop it
								tracing::warn!("Unable to resize image {}, removing it", image.name);
								if let Err(image) = image_receiver.queue_remove(image) {
									tracing::warn!("Unable to remove image {}", image.name);
								}
							}

							// Then try again
							continue 'update;
						},
					},
				}
			}

			break match self.images {
				// If we're empty, try to get a next image
				PanelStateImagesState::Empty => match image_receiver.try_recv() {
					#[allow(clippy::cast_sign_loss)] // It's positive
					Some(image) => {
						handle_image_too_big_error!(
							image,
							self.front_image.update(wgpu, &image, max_image_size),
							PanelStateImagesState::Empty
						);

						self.image_bind_group = self::create_image_bind_group(
							wgpu,
							image_bind_group_layout,
							&self.front_image.texture_view,
							&self.back_image.texture_view,
							&self.texture_sampler,
						);

						(
							PanelStateImagesState::PrimaryOnly {
								front: PanelStateImageState {
									swap_dir: rand::random(),
								},
							},
							// Note: Ensure it's below `0.5` to avoid starting during a fade.
							(rand::random::<f32>() / 2.0 * self.panel.duration as f32) as u64,
						)
					},
					None => (PanelStateImagesState::Empty, 0),
				},

				// If we only have the primary, try to load the next image
				PanelStateImagesState::PrimaryOnly { front } => match image_receiver.try_recv() {
					Some(image) => {
						handle_image_too_big_error!(
							image,
							self.back_image.update(wgpu, &image, max_image_size),
							PanelStateImagesState::PrimaryOnly { front }
						);

						self.image_bind_group = self::create_image_bind_group(
							wgpu,
							image_bind_group_layout,
							&self.front_image.texture_view,
							&self.back_image.texture_view,
							&self.texture_sampler,
						);

						(
							PanelStateImagesState::Both {
								front,
								back: PanelStateImageState {
									swap_dir: rand::random(),
								},
							},
							next_progress,
						)
					},
					None => (PanelStateImagesState::PrimaryOnly { front }, next_progress),
				},

				// If we have both, try to update the progress and swap them if finished
				PanelStateImagesState::Both { mut front, back } if finished => {
					match image_receiver.try_recv() {
						// If we did, stay with both
						Some(image) => {
							handle_image_too_big_error!(
								image,
								self.front_image.update(wgpu, &image, max_image_size),
								PanelStateImagesState::Both { front, back }
							);
							front.swap_dir = rand::random();
							mem::swap(&mut self.front_image, &mut self.back_image);

							self.image_bind_group = self::create_image_bind_group(
								wgpu,
								image_bind_group_layout,
								&self.front_image.texture_view,
								&self.back_image.texture_view,
								&self.texture_sampler,
							);

							(
								PanelStateImagesState::Both {
									front: back,
									back:  front,
								},
								swapped_progress,
							)
						},
						// Else stay on the current progress
						None => (PanelStateImagesState::Both { front, back }, next_progress),
					}
				},

				// Else just update the progress
				state @ PanelStateImagesState::Both { .. } => (state, next_progress),
			};
		};
	}

	/// Calculates this panel's position matrix
	// Note: This matrix simply goes from a geometry in physical units
	//       onto shader coordinates.
	#[must_use]
	pub fn pos_matrix(&self, surface_size: PhysicalSize<u32>) -> Matrix4<f32> {
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

	/// Returns the alpha of the front image
	pub fn front_alpha(&self) -> f32 {
		match self.cur_progress {
			f if f >= self.panel.fade_point =>
				1.0 - (self.cur_progress - self.panel.fade_point) as f32 /
					(self.panel.duration - self.panel.fade_point) as f32,
			_ => 1.0,
		}
	}

	/// Returns all image descriptors to render
	#[must_use]
	pub fn image_descriptors(&self) -> (Option<PanelStateImageDescriptor>, Option<PanelStateImageDescriptor>) {
		// Calculate the progress for the back image
		let back_progress = match self.cur_progress {
			f if f >= self.panel.fade_point =>
				(self.cur_progress - self.panel.fade_point) as f32 / self.panel.duration as f32,
			_ => 0.0,
		};

		// Progress, clamped to `0.0..1.0`
		let progress = self.cur_progress as f32 / self.panel.duration as f32;

		// Get the images to render
		match &self.images {
			PanelStateImagesState::Empty => (None, None),
			PanelStateImagesState::PrimaryOnly { front, .. } => (
				Some(PanelStateImageDescriptor {
					image:    &self.front_image,
					progress: match front.swap_dir {
						true => 1.0 - progress,
						false => progress,
					},
					panel:    &self.panel,
				}),
				None,
			),
			PanelStateImagesState::Both { front, back } => (
				Some(PanelStateImageDescriptor {
					image:    &self.front_image,
					progress: match front.swap_dir {
						true => 1.0 - progress,
						false => progress,
					},
					panel:    &self.panel,
				}),
				Some(PanelStateImageDescriptor {
					image:    &self.back_image,
					progress: match back.swap_dir {
						true => 1.0 - back_progress,
						false => back_progress,
					},
					panel:    &self.panel,
				}),
			),
		}
	}
}

/// State of all images of a panel
#[derive(Clone, Copy, Default, Debug)]
#[allow(clippy::large_enum_variant)] // They'll all progress towards the largest variant
pub enum PanelStateImagesState {
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
		front: PanelStateImageState,
	},

	/// Both
	///
	/// Both images are loaded to be faded in between
	Both {
		/// Front image
		front: PanelStateImageState,

		/// Back image
		back: PanelStateImageState,
	},
}

/// State of a panel's image
#[derive(Clone, Copy, Debug)]
pub struct PanelStateImageState {
	/// If swapping directions for this image
	pub swap_dir: bool,
}

/// Panel image descriptor
#[derive(Clone, Copy, Debug)]
pub struct PanelStateImageDescriptor<'a> {
	/// Image
	pub image: &'a PanelImage,

	/// Progress
	pub progress: f32,

	/// Panel
	pub panel: &'a Panel,
}

impl<'a> PanelStateImageDescriptor<'a> {
	/// Returns the parallax ratio and offset
	pub fn parallax_ratio_offset(&self, ratio: Vector2<f32>, cursor_pos: Point2<i32>) -> (Vector2<f32>, Vector2<f32>) {
		// Matrix to move image outside of the visible parallax scale
		let parallax_offset = {
			let geometry_size = self
				.panel
				.geometry
				.size
				.cast::<f32>()
				.expect("Panel geometry size didn't fit into an `f32`");

			// Calculate the offset from center of image
			let offset = (cursor_pos - self.panel.geometry.center())
				.cast::<f32>()
				.expect("Panel cursor offset didn't fit into an `f32`");

			// Normalize it
			let offset = Vector2::new(2.0 * offset.x / geometry_size.x, 2.0 * offset.y / geometry_size.y);

			// Sign-exponentiate it to make parallax move less near origin
			let offset = Vector2::new(
				offset.x.signum() * offset.x.abs().powf(self.panel.parallax_exp),
				offset.y.signum() * offset.y.abs().powf(self.panel.parallax_exp),
			);

			// Then stretch it to match the ratio
			let offset = Vector2::new(ratio.x * offset.x, ratio.y * offset.y);

			// Then clamp the offset to the edges
			let offset = Vector2::new(offset.x.clamp(-0.5, 0.5), offset.y.clamp(-0.5, 0.5));

			// Then reverse if we should
			let offset = match self.panel.reverse_parallax {
				true => -offset,
				false => offset,
			};

			// Then make sure we don't go more than the parallax ratio allows for
			(1.0 - self.panel.parallax_ratio) * offset
		};

		(
			Vector2::new(self.panel.parallax_ratio, self.panel.parallax_ratio),
			parallax_offset,
		)
	}

	/// Calculates the ratio
	///
	/// This ratio is multiplied by the base uvs to fix the stretching
	/// that comes from having a square coordinate system [0.0 .. 1.0] x [0.0 .. 1.0]
	pub fn ratio(&self) -> Vector2<f32> {
		let image_size = self.image.size.cast().expect("Image size didn't fit into an `i32`");
		let panel_size = self
			.panel
			.geometry
			.size
			.cast()
			.expect("Panel size didn't fit into an `i32`");

		// If either the image or our panel have a side with 0, return a square ratio
		// TODO: Check if this is the right thing to do
		if panel_size.x == 0 || panel_size.y == 0 || image_size.x == 0 || image_size.y == 0 {
			return Vector2::new(0.0, 0.0);
		}

		// Image and panel ratios
		let image_ratio = Rational32::new(image_size.x, image_size.y);
		let panel_ratio = Rational32::new(panel_size.x, panel_size.y);

		// Ratios between the image and panel
		let width_ratio = Rational32::new(panel_size.x, image_size.x);
		let height_ratio = Rational32::new(panel_size.y, image_size.y);

		// X-axis ratio, if image scrolls horizontally
		let x_ratio = self::ratio_as_f32(width_ratio / height_ratio);

		// Y-axis ratio, if image scrolls vertically
		let y_ratio = self::ratio_as_f32(height_ratio / width_ratio);

		match image_ratio >= panel_ratio {
			true => Vector2::new(x_ratio, 1.0),
			false => Vector2::new(1.0, y_ratio),
		}
	}
}

/// Converts a `Ratio<i32>` to `f32`, rounding
// TODO: Although image and window sizes fit into an `f32`, maybe a
//       rational of the two wouldn't fit properly when in a num / denom
//       format, since both may be bigger than `2^24`, check if this is fine.
fn ratio_as_f32(ratio: Rational32) -> f32 {
	*ratio.numer() as f32 / *ratio.denom() as f32
}

/// Creates the texture sampler
fn create_texture_sampler(device: &wgpu::Device) -> wgpu::Sampler {
	let descriptor = wgpu::SamplerDescriptor {
		label: Some("[zsw::panel] Texture sampler"),
		address_mode_u: wgpu::AddressMode::ClampToEdge,
		address_mode_v: wgpu::AddressMode::ClampToEdge,
		address_mode_w: wgpu::AddressMode::ClampToEdge,
		mag_filter: wgpu::FilterMode::Linear,
		min_filter: wgpu::FilterMode::Linear,
		mipmap_filter: wgpu::FilterMode::Linear,
		..wgpu::SamplerDescriptor::default()
	};
	device.create_sampler(&descriptor)
}

/// Creates the texture bind group
fn create_image_bind_group(
	wgpu: &Wgpu,
	bind_group_layout: &wgpu::BindGroupLayout,
	front_view: &wgpu::TextureView,
	back_view: &wgpu::TextureView,
	sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
	let descriptor = wgpu::BindGroupDescriptor {
		layout:  bind_group_layout,
		entries: &[
			wgpu::BindGroupEntry {
				binding:  0,
				resource: wgpu::BindingResource::TextureView(front_view),
			},
			wgpu::BindGroupEntry {
				binding:  1,
				resource: wgpu::BindingResource::TextureView(back_view),
			},
			wgpu::BindGroupEntry {
				binding:  2,
				resource: wgpu::BindingResource::Sampler(sampler),
			},
		],
		label:   None,
	};

	wgpu.device().create_bind_group(&descriptor)
}
