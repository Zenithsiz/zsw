//! Panel geometry

// Imports
use {
	cgmath::{Matrix4, Vector2, Vector3},
	num_rational::Rational32,
	winit::dpi::PhysicalSize,
	zsw_util::Rect,
};


/// Panel geometry
#[derive(Debug)]
pub struct PanelGeometry {
	/// Geometry
	// TODO: Since this is unnormalized for the window, we should
	//       maybe make this private?
	pub geometry: Rect<i32, u32>,
}

impl PanelGeometry {
	pub fn new(geometry: Rect<i32, u32>) -> Self {
		Self { geometry }
	}

	/// Returns this geometry's rectangle for a certain window
	pub fn geometry_on(&self, window_geometry: &Rect<i32, u32>) -> Rect<i32, u32> {
		let mut geometry = self.geometry;
		geometry.pos -= Vector2::new(window_geometry.pos.x, window_geometry.pos.y);

		geometry
	}

	/// Calculates this panel's position matrix
	// Note: This matrix simply goes from a geometry in physical units
	//       onto shader coordinates.
	#[must_use]
	pub fn pos_matrix(&self, window_geometry: &Rect<i32, u32>, surface_size: PhysicalSize<u32>) -> Matrix4<f32> {
		let geometry = self.geometry_on(window_geometry);

		let x_scale = geometry.size[0] as f32 / surface_size.width as f32;
		let y_scale = geometry.size[1] as f32 / surface_size.height as f32;

		let x_offset = geometry.pos[0] as f32 / surface_size.width as f32;
		let y_offset = geometry.pos[1] as f32 / surface_size.height as f32;

		let translation = Matrix4::from_translation(Vector3::new(
			-1.0 + x_scale + 2.0 * x_offset,
			1.0 - y_scale - 2.0 * y_offset,
			0.0,
		));
		let scaling = Matrix4::from_nonuniform_scale(x_scale, -y_scale, 1.0);
		translation * scaling
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
