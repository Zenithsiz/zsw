//! Image uvs

/// Image uvs
///
/// Represents the uvs of an image during it's scroll.
#[derive(Clone, Copy, Debug)]
pub struct ImageUvs {
	/// uvs
	start: [f32; 2],

	/// Swap direction
	swap_dir: bool,
}

impl ImageUvs {
	/// Creates the uvs for an image
	#[must_use]
	pub fn new(image_width: f32, image_height: f32, window_width: f32, window_height: f32, swap_dir: bool) -> Self {
		let start = match image_width / image_height >= window_width / window_height {
			true => [(window_width / image_width) / (window_height / image_height), 1.0],
			false => [1.0, (window_height / image_height) / (window_width / image_width)],
		};

		Self { start, swap_dir }
	}

	/// Returns the starting uvs
	#[must_use]
	pub const fn start(&self) -> [f32; 2] {
		self.start
	}

	/// Returns the offset given progress
	#[must_use]
	pub fn offset(&self, f: f32) -> [f32; 2] {
		let f = match self.swap_dir {
			true => 1.0 - f,
			false => f,
		};

		[f * (1.0 - self.start[0]), f * (1.0 - self.start[1])]
	}
}
