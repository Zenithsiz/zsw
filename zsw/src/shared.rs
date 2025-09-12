//! Shared data

// Imports
use {
	crate::{
		AppEvent,
		Resize,
		display::Displays,
		panel::{Panels, PanelsRendererShared},
		playlist::Playlists,
		profile::Profiles,
	},
	crossbeam::atomic::AtomicCell,
	std::sync::Arc,
	winit::dpi::PhysicalPosition,
	zsw_wgpu::Wgpu,
};

/// Shared data
#[derive(Debug)]
pub struct Shared {
	pub event_loop_proxy: winit::event_loop::EventLoopProxy<AppEvent>,

	pub last_resize: AtomicCell<Option<Resize>>,
	pub cursor_pos:  AtomicCell<Option<PhysicalPosition<f64>>>,

	pub wgpu:                   Wgpu,
	pub panels_renderer_shared: PanelsRendererShared,

	pub displays:  Arc<Displays>,
	pub playlists: Arc<Playlists>,
	pub profiles:  Arc<Profiles>,

	pub panels: Arc<Panels>,
}
