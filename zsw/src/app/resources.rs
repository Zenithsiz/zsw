//! Resources

use zsw_egui::EguiPaintJobsResource;

// Imports
use {
	futures::lock::{Mutex, MutexLockFuture},
	zsw_egui::{EguiPlatformResource, EguiRenderPassResource},
	zsw_panels::PanelsResource,
	zsw_playlist::PlaylistResource,
	zsw_profiles::ProfilesResource,
	zsw_util::ResourcesBundle,
	zsw_wgpu::WgpuSurfaceResource,
};

/// All resources
#[derive(Debug)]
pub struct Resources {
	/// Panels
	pub panels: Mutex<PanelsResource>,

	/// Playlist
	pub playlist: Mutex<PlaylistResource>,

	/// Profiles
	pub profiles: Mutex<ProfilesResource>,

	/// Wgpu surface
	pub wgpu_surface: Mutex<WgpuSurfaceResource>,

	/// Egui platform
	pub egui_platform: Mutex<EguiPlatformResource>,

	/// Egui render pass
	pub egui_render_pass: Mutex<EguiRenderPassResource>,

	/// Egui paint jobs
	pub egui_paint_jobs: Mutex<EguiPaintJobsResource>,
}

impl ResourcesBundle for Resources {}

#[duplicate::duplicate_item(
	ty                 field;
	[ PanelsResource         ] [ panels ];
	[ PlaylistResource       ] [ playlist ];
	[ ProfilesResource       ] [ profiles ];
	[ WgpuSurfaceResource    ] [ wgpu_surface ];
	[ EguiPlatformResource   ] [ egui_platform ];
	[ EguiRenderPassResource ] [ egui_render_pass ];
	[ EguiPaintJobsResource  ] [ egui_paint_jobs ];
)]
impl zsw_util::Resources<ty> for Resources {
	fn lock(&self) -> MutexLockFuture<ty> {
		self.field.lock()
	}
}
