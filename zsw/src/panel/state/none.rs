//! Panel none state

/// Panel none state
#[derive(Debug)]
pub struct PanelNoneState {
	/// Background color
	pub background_color: [f32; 4],
}

impl PanelNoneState {
	/// Creates new state
	pub fn new(background_color: [f32; 4]) -> Self {
		Self { background_color }
	}
}
