//! Rect

// Imports
use anyhow::Context;
use cgmath::{num_traits::Num, Point2, Vector2};
use std::{error::Error, fmt};

/// A rectangle
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub struct Rect<T> {
	/// Position
	pub pos: Point2<T>,

	/// Size
	pub size: Vector2<T>,
}

impl<T> Rect<T> {
	/// Parses a rect from a geometry, `{width}x{height}+{x}+{y}` or `{width}x{height}`
	#[allow(clippy::missing_errors_doc)] // Fails it not in the specified format
	pub fn parse_from_geometry(s: &str) -> Result<Self, anyhow::Error>
	where
		T: Num,
		<T as Num>::FromStrRadixErr: 'static + Send + Sync + Error,
	{
		// Split at the first `+`, or just use it all, if there's no position
		let (size, pos) = s
			.split_once('+')
			.map_or((s, None), |(height, rest)| (height, Some(rest)));

		// Split at the first `x` to get the width and height
		let (width, height) = size.split_once('x').context("Unable to find `x` in size")?;

		let size = Vector2::new(
			T::from_str_radix(width, 10).context("Unable to parse width")?,
			T::from_str_radix(height, 10).context("Unable to parse height")?,
		);

		// Optionally get the position if it exists
		let pos = match pos {
			Some(s) => {
				let (x, y) = s.split_once('+').context("Unable to find `+` in position")?;
				Point2::new(
					T::from_str_radix(x, 10).context("Unable to parse x")?,
					T::from_str_radix(y, 10).context("Unable to parse y")?,
				)
			},
			None => Point2::new(T::zero(), T::zero()),
		};

		Ok(Self { pos, size })
	}
}

impl fmt::Display for Rect<u32> {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}x{}", self.size.x, self.size.y)?;

		if self.pos.x != 0 || self.pos.y != 0 {
			write!(f, "+{}+{}", self.pos.x, self.pos.y)?;
		}

		Ok(())
	}
}
