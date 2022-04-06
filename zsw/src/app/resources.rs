//! Resources

// Imports
use {
	futures::lock::{Mutex, MutexLockFuture},
	zsw_panels::PanelsResource,
	zsw_playlist::PlaylistResource,
	zsw_profiles::ProfilesResource,
	zsw_util::{ResourcesBundle, ResourcesLock},
	zsw_wgpu::WgpuSurfaceResource,
};

/// All resources
pub struct Resources {
	/// Panels
	pub panels: Mutex<PanelsResource>,

	/// Playlist
	pub playlist: Mutex<PlaylistResource>,

	/// Profiles
	pub profiles: Mutex<ProfilesResource>,

	/// Wgpu surface
	pub wgpu_surface: Mutex<WgpuSurfaceResource>,
}

impl ResourcesBundle for Resources {}

#[duplicate::duplicate_item(
	ty                 field;
	[ PanelsResource      ] [ panels ];
	[ PlaylistResource    ] [ playlist ];
	[ ProfilesResource    ] [ profiles ];
	[ WgpuSurfaceResource ] [ wgpu_surface ];
)]
impl ResourcesLock<ty> for Resources {
	fn lock(&self) -> MutexLockFuture<ty> {
		self.field.lock()
	}
}
