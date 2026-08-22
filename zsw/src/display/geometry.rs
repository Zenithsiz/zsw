//! Display geometry

// Imports
use {
	euclid::default::{Transform3D, Vector2D},
	num_rational::Rational32,
	winit::dpi::PhysicalSize,
	zsw_util::Rect,
};

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub struct DisplayGeometry {
	/// Inner geometry
	inner: Rect<i32, u32>,
}

impl DisplayGeometry {
	/// Creates a new geometry
	pub fn new(inner: Rect<i32, u32>) -> Self {
		Self { inner }
	}

	/// Gets the inner rectangle
	pub fn rect(&self) -> Rect<i32, u32> {
		self.inner
	}

	/// Returns if this geometry intersects a window
	pub fn intersects_window(&self, window_geometry: Rect<i32, u32>) -> bool {
		self.inner.intersection(window_geometry).is_some()
	}

	/// Returns this geometry's rectangle for a certain window
	pub fn on_window(&self, window_geometry: Rect<i32, u32>) -> Rect<i32, u32> {
		let mut geometry = self.inner;
		geometry.pos -= Vector2D::new(window_geometry.pos.x, window_geometry.pos.y);

		geometry
	}

	/// Calculates this panel's position matrix
	// Note: This matrix simply goes from a geometry in physical units
	//       onto shader coordinates.
	#[must_use]
	pub fn pos_matrix(&self, window_geometry: Rect<i32, u32>, surface_size: PhysicalSize<u32>) -> Transform3D<f32> {
		let geometry = self.on_window(window_geometry);

		let x_scale = geometry.size.x as f32 / surface_size.width as f32;
		let y_scale = geometry.size.y as f32 / surface_size.height as f32;

		let x_offset = geometry.pos.x as f32 / surface_size.width as f32;
		let y_offset = geometry.pos.y as f32 / surface_size.height as f32;

		let translation =
			Transform3D::translation(-1.0 + x_scale + 2.0 * x_offset, 1.0 - y_scale - 2.0 * y_offset, 0.0);
		translation.pre_scale(x_scale, -y_scale, 1.0)
	}

	/// Calculates an image's ratio for this panel geometry
	///
	/// This ratio is multiplied by the base uvs to fix the stretching
	/// that comes from having a square coordinate system [0.0 .. 1.0] x [0.0 .. 1.0]
	pub fn image_ratio(&self, image_size: Vector2D<u32>) -> Vector2D<f32> {
		let image_size = image_size.cast();
		let panel_size = self.inner.size.cast();

		// If either the image or our panel have a side with 0, return a square ratio
		// TODO: Check if this is the right thing to do
		if panel_size.x == 0 || panel_size.y == 0 || image_size.x == 0 || image_size.y == 0 {
			return Vector2D::new(0.0, 0.0);
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
			true => Vector2D::new(x_ratio, 1.0),
			false => Vector2D::new(1.0, y_ratio),
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
