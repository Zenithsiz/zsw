//! Lockers

// Imports
use {
	super::{locker::Resource, Locker},
	crate::panel::{PanelGroup, PanelsRendererShader},
	futures::lock::Mutex,
	std::sync::Arc,
	zsw_util::where_assert,
};

// Note: `duplicate` is just being used to create aliases here
#[duplicate::duplicate_item(
	t0 T0 t1 T1;

	[ cur_panel_group        ] [ Option<PanelGroup> ]
	[ panels_renderer_shader ] [ PanelsRendererShader ];
)]
define_locker! {
	LoadDefaultPanelGroupLocker {
		fn new(...) -> Self;
		fn resource(...) -> ...;

		t0: T0 = 0,
	}

	RendererLocker {
		fn new(...) -> Self;
		fn resource(...) -> ...;

		t0: T0 = 0,
		t1: T1 = 1,
	}

	PanelsUpdaterLocker {
		fn new(...) -> Self;
		fn resource(...) -> ...;

		t0: T0 = 0,
	}

	EguiPainterLocker {
		fn new(...) -> Self;
		fn resource(...) -> ...;

		t0: T0 = 0,
		t1: T1 = 1,
	}
}

macro define_locker(
	$(
		$LockerName:ident {
			fn $new:ident(...) -> Self;
			fn $resource:ident(...) -> ...;

			$(
				$lock_name:ident: $lock_ty:ty = $lock_idx:literal
			),*
			$(,)?
		}
	)*

) {
	$(
		/// Locker
		///
		/// # Thread safety
		/// Multiple lockers should not be created per task
		#[derive(Debug)]
		pub struct $LockerName<const STATE: usize = 0> {
			$(
				$lock_name: Arc<Mutex<$lock_ty>>,
			)*
		}

		impl<const STATE: usize> $LockerName<STATE> {
			/// Creates a new locker
			pub fn $new($( $lock_name: Arc<Mutex<$lock_ty>> ),*) -> Self {
				Self { $( $lock_name, )* }
			}

			/// Locks a resource
			#[track_caller]
			pub async fn $resource<'locker, R>(
				&'locker mut self,
			) -> (Resource<'locker, R>, <Self as Locker<R>>::Next<'locker>)
			where
				Self: Locker<R>,
				R: 'locker,
			{
				self.lock_resource().await
			}
		}

		#[duplicate::duplicate_item(
			ResourceTy field NEXT_STATE;
			$(
				[$lock_ty] [$lock_name] [{ $lock_idx + 1 }];
			)*
		)]
		impl<const CUR_STATE: usize> Locker<ResourceTy> for $LockerName<CUR_STATE>
		where
			where_assert!(NEXT_STATE > CUR_STATE):,
		{
			type Next<'locker> = $LockerName<NEXT_STATE>;

			#[track_caller]
			async fn lock_resource<'locker>(&'locker mut self) -> (Resource<ResourceTy>, Self::Next<'locker>)
			where
				ResourceTy: 'locker,
			{
				#[cfg(feature = "locker-trace")]
				tracing::trace!(resource = ?std::any::type_name::<ResourceTy>(), backtrace = %std::backtrace::Backtrace::force_capture(), "Locking resource");
				let guard = self.field.lock().await;

				#[cfg(feature = "locker-trace")]
				tracing::trace!(resource = ?std::any::type_name::<ResourceTy>(), backtrace = %std::backtrace::Backtrace::force_capture(), "Locked resource");
				let resource = Resource::new(guard);

				// TODO: Not clone an `Arc` here each time.
				let locker = $LockerName {
					$(
						$lock_name: Arc::clone(&self.$lock_name),
					)*
				};
				(resource, locker)
			}
		}
	)*
}
