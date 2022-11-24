//! Resources

// Imports
use {
	futures::lock::{Mutex, MutexGuard},
	zsw_panels::PanelsResource,
	zsw_util::ResourcesBundle,
	zsw_wgpu::WgpuSurfaceResource,
};

/// All resources
#[derive(Debug)]
pub struct Resources {
	/// Panels
	pub panels: Mutex<PanelsResource>,

	/// Wgpu surface
	pub wgpu_surface: Mutex<WgpuSurfaceResource>,
}

impl ResourcesBundle for Resources {}

#[duplicate::duplicate_item(
	ty                 field;
	[ PanelsResource         ] [ panels ];
	[ WgpuSurfaceResource    ] [ wgpu_surface ];
)]
impl zsw_util::Resources<ty> for Resources {
	type Resource<'a> = MutexGuard<'a, ty>;

	async fn lock(&self) -> Self::Resource<'_> {
		self.field.lock().await
	}
}

#[duplicate::duplicate_item(
	ty1                   val1             ty2                val2    ;
	[WgpuSurfaceResource] [ wgpu_surface ] [ PanelsResource ] [panels];
)]
const _: () = {
	// Main impl
	impl zsw_util::ResourcesTuple2<ty1, ty2> for Resources {
		type Resources1<'a> = MutexGuard<'a, ty1>;
		type Resources2<'a> = MutexGuard<'a, ty2>;

		async fn lock(&self) -> (Self::Resources1<'_>, Self::Resources2<'_>) {
			let val1 = self.val1.lock().await;
			let val2 = self.val2.lock().await;

			(val1, val2)
		}
	}

	// Reverse impl (with same locking order)
	// Note: Can't be a blanket impl (until specialization ?)
	impl zsw_util::ResourcesTuple2<ty2, ty1> for Resources {
		type Resources1<'a> = <Self as zsw_util::ResourcesTuple2<ty1, ty2>>::Resources2<'a>;
		type Resources2<'a> = <Self as zsw_util::ResourcesTuple2<ty1, ty2>>::Resources1<'a>;

		async fn lock(&self) -> (Self::Resources1<'_>, Self::Resources2<'_>) {
			let (val1, val2) = <Self as zsw_util::ResourcesTuple2<ty1, ty2>>::lock(self).await;
			(val2, val1)
		}
	}
};
