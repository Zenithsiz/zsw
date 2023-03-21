//! Shared data

// Modules
mod locker;

// Exports
pub use self::locker::{
	AsyncLocker,
	AsyncMutexResource,
	AsyncRwLockResource,
	CurPanelGroupMutex,
	EguiPainterRendererMeetupSender,
	LockerIteratorExt,
	LockerStreamExt,
	MeetupSenderResource,
	PanelsRendererShaderRwLock,
	PanelsUpdaterMeetupSender,
	PlaylistItemRwLock,
	PlaylistRwLock,
	PlaylistsRwLock,
};

// Imports
use {
	crate::{
		image_loader::ImageRequester,
		panel::{PanelsManager, PanelsRendererLayouts},
		playlist::PlaylistsManager,
		Resize,
	},
	crossbeam::atomic::AtomicCell,
	std::sync::Arc,
	winit::dpi::PhysicalPosition,
	zsw_wgpu::WgpuShared,
};

/// Shared data
#[derive(Debug)]
pub struct Shared {
	// TODO: Not have a double-`Arc` in here?
	pub window:                 Arc<winit::window::Window>,
	pub wgpu:                   WgpuShared,
	pub panels_renderer_layout: PanelsRendererLayouts,
	pub last_resize:            AtomicCell<Option<Resize>>,
	pub cursor_pos:             AtomicCell<PhysicalPosition<f64>>,

	pub panels_manager:    PanelsManager,
	pub image_requester:   ImageRequester,
	pub playlists_manager: PlaylistsManager,

	pub cur_panel_group:        CurPanelGroupMutex,
	pub panels_renderer_shader: PanelsRendererShaderRwLock,
	pub playlists:              PlaylistsRwLock,
}
