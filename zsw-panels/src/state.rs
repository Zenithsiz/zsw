//! Panel state

// Imports
use {
	super::PanelImage,
	crate::{Panel, PanelsRenderer},
	cgmath::{Matrix4, Point2, Vector2, Vector3},
	num_rational::Rational32,
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
					image: front,
					alpha: 1.0,
					progress,
					panel: &self.panel,
				}),
				None,
			),
			PanelStateImages::Both { front, back } => (
				Some(PanelStateImageDescriptor {
					image: front,
					alpha: 1.0 - back_alpha,
					progress,
					panel: &self.panel,
				}),
				Some(PanelStateImageDescriptor {
					image:    back,
					alpha:    back_alpha,
					progress: back_progress,
					panel:    &self.panel,
				}),
			),
		};

		[front, back]
			.into_iter()
			.flatten()
			.filter(|descriptor| descriptor.alpha != 0.0)
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
	image: &'a PanelStateImage,

	/// Alpha
	alpha: f32,

	/// Progress
	progress: f32,

	/// Panel
	panel: &'a Panel,
}

impl<'a> PanelStateImageDescriptor<'a> {
	/// Calculates this image's uvs matrix.
	#[must_use]
	pub fn uvs_matrix(&self, cursor_pos: Point2<i32>) -> Matrix4<f32> {
		// Provides the correct ratio for the image
		let ratio = self.ratio();
		let ratio_scalar = Matrix4::from_nonuniform_scale(ratio.x, ratio.y, 1.0);

		// Offsets the image due to it's progress
		let offset = self.offset(ratio);
		let progress_offset = Matrix4::from_translation(Vector3::new(offset.x, offset.y, 0.0));

		// Base matrix
		let base_matrix = progress_offset * ratio_scalar;

		// Calculate the parallax matrix
		let parallax_matrix = self.parallax_matrix(ratio, cursor_pos);

		// Then add it to our setup
		parallax_matrix * base_matrix
	}

	/// Calculates the parallax matrix
	///
	/// This matrix will add parallax to the existing matrix setup
	fn parallax_matrix(&self, ratio: Vector2<f32>, cursor_pos: Point2<i32>) -> Matrix4<f32> {
		// Matrices to move image center to origin, and then back
		let middle_pos = Vector3::new(ratio.x / 2.0, ratio.y / 2.0, 0.0);
		let move_origin = Matrix4::from_translation(-middle_pos);
		let move_back = Matrix4::from_translation(middle_pos);

		// Matrix to scale the image down so we can add later movement
		let scalar = Matrix4::from_nonuniform_scale(self.panel.parallax_ratio, self.panel.parallax_ratio, 1.0);

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
		let move_parallax = Matrix4::from_translation(Vector3::new(parallax_offset.x, parallax_offset.y, 0.0));

		// Center image on origin, scale it, move it by parallax and move it back
		move_back * move_parallax * scalar * move_origin
	}

	/// Calculates the offset
	///
	/// This offset serve to scroll the image depending on our progress.
	fn offset(&self, ratio_uvs: Vector2<f32>) -> Vector2<f32> {
		// If we're going backwards, invert progress
		let progress = match self.image.swap_dir {
			true => 1.0 - self.progress,
			false => self.progress,
		};

		// Then simply offset until the end
		Vector2::new(progress * (1.0 - ratio_uvs.x), progress * (1.0 - ratio_uvs.y))
	}

	/// Calculates the ratio
	///
	/// This ratio is multiplied by the base uvs to fix the stretching
	/// that comes from having a square coordinate system [0.0 .. 1.0] x [0.0 .. 1.0]
	fn ratio(&self) -> Vector2<f32> {
		let image_size = self
			.image
			.image
			.size()
			.cast()
			.expect("Image size didn't fit into an `i32`");
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

	/// Returns the alpha
	pub fn alpha(&self) -> f32 {
		self.alpha
	}

	/// Returns the image to render
	pub fn image(&self) -> &'a PanelImage {
		&self.image.image
	}
}

/// Converts a `Ratio<i32>` to `f32`, rounding
// TODO: Although image and window sizes fit into an `f32`, maybe a
//       rational of the two wouldn't fit properly when in a num / denom
//       format, since both may be bigger than `2^24`, check if this is fine.
fn ratio_as_f32(ratio: Rational32) -> f32 {
	*ratio.numer() as f32 / *ratio.denom() as f32
}
