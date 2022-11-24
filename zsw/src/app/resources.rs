//! Resources

// Imports
use {
	futures::lock::{Mutex, MutexGuard},
	std::{future::Future, sync::Arc},
	zsw_panels::PanelsResource,
	zsw_util::ResourcesBundle,
	zsw_wgpu::WgpuSurfaceResource,
};

/// Resources inner storage
#[derive(Debug)]
pub struct ResourcesInner {
	/// Panels
	pub panels: Mutex<PanelsResource>,

	/// Wgpu surface
	pub wgpu_surface: Mutex<WgpuSurfaceResource>,
}

/// Resources
// TODO: Remove this wrapper once we can impl `Resources` on `&'a ResourcesInner`
//       Or once we can impl on `Arc<ResourcesInner>` (if it becomes fundamental)
#[derive(Clone, Debug)]
pub struct Resources(pub Arc<ResourcesInner>);

impl ResourcesBundle for Resources {}

#[duplicate::duplicate_item(
	ty                 field;
	[ PanelsResource         ] [ panels ];
	[ WgpuSurfaceResource    ] [ wgpu_surface ];
)]
impl zsw_util::Resources<ty> for Resources {
	type Resource<'a> = MutexGuard<'a, ty>
	where
		Self: 'a;

	type LockFuture<'a> = impl Future<Output = Self::Resource<'a>>
	where
		Self: 'a;

	fn lock(&mut self) -> Self::LockFuture<'_> {
		async move { self.0.field.lock().await }
	}
}

#[duplicate::duplicate_item(
	ty1                   val1             ty2                val2    ;
	[WgpuSurfaceResource] [ wgpu_surface ] [ PanelsResource ] [panels];
)]
const _: () = {
	// Main impl
	impl zsw_util::ResourcesTuple2<ty1, ty2> for Resources {
		type Resources1<'a> = MutexGuard<'a, ty1>
		where
			Self: 'a;
		type Resources2<'a> = MutexGuard<'a, ty2>
		where
			Self: 'a;

		type LockFuture<'a> = impl Future<Output = (Self::Resources1<'a>, Self::Resources2<'a>)>
		where
			Self: 'a;

		fn lock(&mut self) -> Self::LockFuture<'_> {
			async move {
				let val1 = self.0.val1.lock().await;
				let val2 = self.0.val2.lock().await;

				(val1, val2)
			}
		}
	}

	// Reverse impl (with same locking order)
	// Note: Can't be a blanket impl (until specialization ?)
	impl zsw_util::ResourcesTuple2<ty2, ty1> for Resources {
		type Resources1<'a> = <Self as zsw_util::ResourcesTuple2<ty1, ty2>>::Resources2<'a>
		where
			Self: 'a;
		type Resources2<'a> = <Self as zsw_util::ResourcesTuple2<ty1, ty2>>::Resources1<'a>
		where
			Self: 'a;

		type LockFuture<'a> = impl Future<Output = (Self::Resources1<'a>, Self::Resources2<'a>)>
			where
				Self: 'a;

		fn lock(&mut self) -> Self::LockFuture<'_> {
			async move {
				let (val1, val2) = <Self as zsw_util::ResourcesTuple2<ty1, ty2>>::lock(self).await;
				(val2, val1)
			}
		}
	}
};
