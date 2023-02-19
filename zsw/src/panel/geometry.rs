//! Panel geometry

use super::PanelsRendererLayouts;

// Imports
use {
	crate::wgpu_wrapper::WgpuShared,
	cgmath::{Matrix4, Point2, Vector2, Vector3},
	num_rational::Rational32,
	wgpu::util::DeviceExt,
	winit::dpi::PhysicalSize,
	zsw_util::Rect,
};


/// Panel geometry
#[derive(Debug)]
pub struct PanelGeometry {
	/// Geometry
	pub geometry: Rect<i32, u32>,

	/// Uniforms
	pub uniforms: wgpu::Buffer,

	/// Uniforms bind group
	pub uniforms_bind_group: wgpu::BindGroup,
}

impl PanelGeometry {
	pub fn new(wgpu_shared: &WgpuShared, renderer_layouts: &PanelsRendererLayouts, geometry: Rect<i32, u32>) -> Self {
		// Create the uniforms
		// Note: Initial value doesn't matter
		let uniforms_descriptor = wgpu::util::BufferInitDescriptor {
			label:    None,
			// TODO: Resize buffer as we go?
			contents: &[0; 0x100],
			usage:    wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
		};
		let uniforms = wgpu_shared.device.create_buffer_init(&uniforms_descriptor);

		// Create the uniform bind group
		let uniforms_bind_group_descriptor = wgpu::BindGroupDescriptor {
			layout:  &renderer_layouts.uniforms_bind_group_layout,
			entries: &[wgpu::BindGroupEntry {
				binding:  0,
				resource: uniforms.as_entire_binding(),
			}],
			label:   None,
		};
		let uniforms_bind_group = wgpu_shared.device.create_bind_group(&uniforms_bind_group_descriptor);

		Self {
			geometry,
			uniforms,
			uniforms_bind_group,
		}
	}

	/// Calculates this panel's position matrix
	// Note: This matrix simply goes from a geometry in physical units
	//       onto shader coordinates.
	#[must_use]
	pub fn pos_matrix(&self, surface_size: PhysicalSize<u32>) -> Matrix4<f32> {
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

	/// Returns the parallax ratio and offset
	pub fn parallax_ratio_offset(
		&self,
		ratio: Vector2<f32>,
		cursor_pos: Point2<i32>,
		parallax_ratio: f32,
		parallax_exp: f32,
		reverse_parallax: bool,
	) -> (Vector2<f32>, Vector2<f32>) {
		// Matrix to move image outside of the visible parallax scale
		let parallax_offset = {
			let geometry_size = self
				.geometry
				.size
				.cast::<f32>()
				.expect("Panel geometry size didn't fit into an `f32`");

			// Calculate the offset from center of image
			let offset = (cursor_pos - self.geometry.center())
				.cast::<f32>()
				.expect("Panel cursor offset didn't fit into an `f32`");

			// Normalize it
			let offset = Vector2::new(2.0 * offset.x / geometry_size.x, 2.0 * offset.y / geometry_size.y);

			// Sign-exponentiate it to make parallax move less near origin
			let offset = Vector2::new(
				offset.x.signum() * offset.x.abs().powf(parallax_exp),
				offset.y.signum() * offset.y.abs().powf(parallax_exp),
			);

			// Then stretch it to match the ratio
			let offset = Vector2::new(ratio.x * offset.x, ratio.y * offset.y);

			// Then clamp the offset to the edges
			let offset = Vector2::new(offset.x.clamp(-0.5, 0.5), offset.y.clamp(-0.5, 0.5));

			// Then reverse if we should
			let offset = match reverse_parallax {
				true => -offset,
				false => offset,
			};

			// Then make sure we don't go more than the parallax ratio allows for
			(1.0 - parallax_ratio) * offset
		};

		(Vector2::new(parallax_ratio, parallax_ratio), parallax_offset)
	}

	/// Calculates an image's ratio for this panel geometry
	///
	/// This ratio is multiplied by the base uvs to fix the stretching
	/// that comes from having a square coordinate system [0.0 .. 1.0] x [0.0 .. 1.0]
	pub fn image_ratio(panel_size: Vector2<u32>, image_size: Vector2<u32>) -> Vector2<f32> {
		let image_size = image_size.cast().expect("Image size didn't fit into an `i32`");
		let panel_size = panel_size.cast().expect("Panel size didn't fit into an `i32`");

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
