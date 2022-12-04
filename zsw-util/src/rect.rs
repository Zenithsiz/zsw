//! Rect

// Imports
use {
	anyhow::Context,
	cgmath::{Point2, Vector2},
	std::{borrow::Cow, fmt, str::FromStr},
};

/// A rectangle
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub struct Rect<P, S = P> {
	/// Position
	pub pos: Point2<P>,

	/// Size
	pub size: Vector2<S>,
}

impl Rect<i32, u32> {
	/// Returns a rect with 0 in position and size
	#[must_use]
	pub fn zero() -> Self {
		Self {
			pos:  Point2::new(0, 0),
			size: Vector2::new(0, 0),
		}
	}

	/// Merges two rectangles and returns a rectangle containing both
	#[must_use]
	pub fn merge(self, rhs: Self) -> Self {
		let lhs = self;

		// Get the min/max of each
		let lhs_min = lhs.min();
		let rhs_min = rhs.min();
		let lhs_max = lhs.max();
		let rhs_max = rhs.max();

		// Clamp them to enclose them all
		let merged_min = Point2::new(lhs_min.x.min(rhs_min.x), lhs_min.y.min(rhs_min.y));
		let merged_max = Point2::new(lhs_max.x.max(rhs_max.x), lhs_max.y.max(rhs_max.y));

		// Then reconstruct
		Self::from_min_max(merged_min, merged_max)
	}

	/// Creates a rectangle from min/max
	#[must_use]
	pub fn from_min_max(min: Point2<i32>, max: Point2<i32>) -> Self {
		Self {
			pos:  min,
			size: (max - min).cast().expect("Unable to cast"),
		}
	}

	/// Returns the min position of this rectangle
	#[must_use]
	pub fn min(self) -> Point2<i32> {
		self.pos
	}

	/// Returns the max position of this rectangle
	#[must_use]
	pub fn max(self) -> Point2<i32> {
		Point2::new(
			self.pos.x.checked_add_unsigned(self.size.x).expect("Overflow"),
			self.pos.y.checked_add_unsigned(self.size.y).expect("Overflow"),
		)
	}

	/// Returns the center of this rectangle
	#[must_use]
	pub fn center(self) -> Point2<i32> {
		Point2::new(
			self.pos.x.checked_add_unsigned(self.size.x / 2).expect("Overflow"),
			self.pos.y.checked_add_unsigned(self.size.y / 2).expect("Overflow"),
		)
	}
}

impl fmt::Display for Rect<i32, u32> {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}x{}", self.size.x, self.size.y)?;

		if self.pos.x != 0 || self.pos.y != 0 {
			write!(f, "+{}+{}", self.pos.x, self.pos.y)?;
		}

		Ok(())
	}
}

impl FromStr for Rect<i32, u32> {
	type Err = anyhow::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		// Split at the first `+`, or just use it all, if there's no position
		let (size, pos) = s
			.split_once('+')
			.map_or((s, None), |(height, rest)| (height, Some(rest)));

		// Split at the first `x` to get the width and height
		let (width, height) = size.split_once('x').context("Unable to find `x` in size")?;

		let size = Vector2::new(
			width.parse::<u32>().context("Unable to parse width")?,
			height.parse::<u32>().context("Unable to parse height")?,
		);

		// Optionally get the position if it exists
		let pos = match pos {
			Some(s) => {
				let (x, y) = s.split_once('+').context("Unable to find `+` in position")?;
				Point2::new(
					x.parse::<i32>().context("Unable to parse x")?,
					y.parse::<i32>().context("Unable to parse y")?,
				)
			},
			None => Point2::new(0, 0),
		};

		Ok(Self { pos, size })
	}
}

impl<'de> serde::Deserialize<'de> for Rect<i32, u32> {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		let s = Cow::deserialize(deserializer)?;
		Self::from_str(&s).map_err(serde::de::Error::custom)
	}
}

impl serde::Serialize for Rect<i32, u32> {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		serializer.serialize_str(&self.to_string())
	}
}
