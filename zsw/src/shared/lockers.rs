//! Lockers

// Imports
use {
	super::AsyncMutexLocker,
	crate::panel::{PanelGroup, PanelsRendererShader},
	futures::lock::{Mutex, MutexGuard},
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
		inner;
		fn new(...) -> Self;
		fn lock(...) -> ...;

		t0: T0 = 0,
	}

	RendererLocker {
		inner;
		fn new(...) -> Self;
		fn lock(...) -> ...;

		t0: T0 = 0,
		t1: T1 = 1,
	}

	PanelsUpdaterLocker {
		inner;
		fn new(...) -> Self;
		fn lock(...) -> ...;

		t0: T0 = 0,
	}

	EguiPainterLocker {
		inner;
		fn new(...) -> Self;
		fn lock(...) -> ...;

		t0: T0 = 0,
		t1: T1 = 1,
	}
}

macro define_locker(
	$(
		$LockerName:ident {
			$inner:ident;
			fn $new:ident(...) -> Self;
			fn $lock_fn:ident(...) -> ...;

			$(
				$lock_name:ident: $lock_ty:ty = $lock_idx:literal
			),*
			$(,)?
		}
	)*

) {
	paste::paste! {
		$(
			/// Locker inner
			#[derive(Debug)]
			pub struct [< $LockerName Inner >] {
				$(
					$lock_name: Arc<Mutex<$lock_ty>>,
				)*
			}

			/// Locker
			///
			/// # Thread safety
			/// Multiple lockers should not be created per task
			#[derive(Debug)]
			pub struct $LockerName<const STATE: usize = 0> {
				$inner: &'static [< $LockerName Inner >],
			}

			impl<const STATE: usize> $LockerName<STATE> {
				/// Creates a new locker
				pub fn $new($( $lock_name: Arc<Mutex<$lock_ty>> ),*) -> Self {
					// TODO: Don't leak and instead drop it only when dropping when `STATE == 0`.
					let inner = [< $LockerName Inner >] {
						$( $lock_name, )*
					};
					let inner = Box::leak(Box::new(inner));

					Self { $inner: inner }
				}

				/// Locks the resource `R`
				#[track_caller]
				pub async fn $lock_fn<'locker, R>(
					&'locker mut self,
				) -> (MutexGuard<'locker, R>, <Self as AsyncMutexLocker<R>>::Next<'locker>)
				where
					Self: AsyncMutexLocker<R>,
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
			impl<const CUR_STATE: usize> AsyncMutexLocker<ResourceTy> for $LockerName<CUR_STATE>
			where
				where_assert!(NEXT_STATE > CUR_STATE):,
			{
				type Next<'locker> = $LockerName<NEXT_STATE>;

				#[track_caller]
				async fn lock_resource<'locker>(&'locker mut self) -> (MutexGuard<ResourceTy>, Self::Next<'locker>)
				where
					ResourceTy: 'locker,
				{
					let guard = self.$inner.field.lock().await;

					let locker = $LockerName {
						$inner: self.$inner
					};
					(guard, locker)
				}
			}
		)*
	}
}
