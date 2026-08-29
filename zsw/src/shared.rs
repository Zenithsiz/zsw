//! Shared data

use {std::sync::Arc, winit::window::Window, zsw_util::Rect};

/// Shared window data
#[derive(Debug)]
pub struct SharedWindow {
	/// Window
	pub window: Arc<Window>,

	/// Monitor name
	pub monitor_name: String,

	/// Monitor geometry
	pub monitor_geometry: Rect<i32, u32>,

	/// Monitor refresh rate (in mHz)
	pub monitor_refresh_rate_mhz: u32,
}
