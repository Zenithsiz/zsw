//! Shared data

// Imports
use {
	crate::{
		AppEvent,
		Resize,
		display::Displays,
		metrics::Metrics,
		panel::{Panels, PanelsRendererShared},
		playlist::Playlists,
		profile::Profiles,
	},
	crossbeam::atomic::AtomicCell,
	tokio::sync::Mutex,
	std::sync::Arc,
	winit::{dpi::PhysicalPosition, event_loop::EventLoopProxy},
	zsw_wgpu::Wgpu,
};

/// Shared data
#[derive(Debug)]
pub struct Shared {
	pub event_loop_proxy: EventLoopProxy<AppEvent>,

	pub last_resize: AtomicCell<Option<Resize>>,
	pub cursor_pos:  AtomicCell<Option<PhysicalPosition<f64>>>,

	pub wgpu:                   Wgpu,
	pub panels_renderer_shared: Mutex<PanelsRendererShared>,

	pub displays:  Arc<Displays>,
	pub playlists: Arc<Playlists>,
	pub profiles:  Arc<Profiles>,

	pub panels: Arc<Panels>,

	pub metrics: Metrics,
}
