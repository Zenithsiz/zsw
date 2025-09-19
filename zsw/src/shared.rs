//! Shared data

// Imports
use {
	crate::{
		AppEvent,
		display::Displays,
		metrics::Metrics,
		panel::{Panels, PanelsRendererShared},
		playlist::Playlists,
		profile::Profiles,
	},
	std::{collections::HashMap, sync::Arc},
	tokio::sync::Mutex,
	winit::{
		event_loop::EventLoopProxy,
		window::{Window, WindowId},
	},
	zsw_util::Rect,
	zsw_wgpu::Wgpu,
};

/// Shared data
#[derive(Debug)]
pub struct Shared {
	pub event_loop_proxy: EventLoopProxy<AppEvent>,

	pub wgpu:                   Wgpu,
	pub panels_renderer_shared: PanelsRendererShared,

	pub displays:  Arc<Displays>,
	pub playlists: Arc<Playlists>,
	pub profiles:  Arc<Profiles>,

	pub panels: Arc<Panels>,

	pub metrics: Metrics,

	pub windows: Mutex<HashMap<WindowId, Arc<SharedWindow>>>,
}

/// Shared window data
#[derive(Debug)]
pub struct SharedWindow {
	/// Window
	pub window: Arc<Window>,

	/// Monitor name
	pub monitor_name: String,

	/// Monitor geometry
	pub monitor_geometry: Mutex<Rect<i32, u32>>,

	/// Monitor refresh rate (in mHz)
	pub monitor_refresh_rate_mhz: u32,
}
