//! Profile panel geometry

use zsw_util::Rect;

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub struct ProfilePanelGeometry {
	/// Inner geometry
	pub geometry: Rect<i32, u32>,
}
