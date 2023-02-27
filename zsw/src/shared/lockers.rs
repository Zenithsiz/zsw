//! Lockers

// Imports
use {
	super::locker::AsyncMutexLocker,
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

		async_mutex {
			t0: T0 = 0,
		}
	}

	RendererLocker {
		inner;
		fn new(...) -> Self;
		fn lock(...) -> ...;

		async_mutex {
			t0: T0 = 0,
			t1: T1 = 1,
		}
	}

	PanelsUpdaterLocker {
		inner;
		fn new(...) -> Self;
		fn lock(...) -> ...;

		async_mutex {
			t0: T0 = 0,
		}
	}

	EguiPainterLocker {
		inner;
		fn new(...) -> Self;
		fn lock(...) -> ...;

		async_mutex {
			t0: T0 = 0,
			t1: T1 = 1,
		}
	}
}

macro define_locker(
	$(
		$LockerName:ident {
			$inner:ident;
			fn $new:ident(...) -> Self;
			fn $lock_async_mutex:ident(...) -> ...;

			$(
				async_mutex {
					$(
						$async_mutex_name:ident: $async_mutex_ty:ty = $async_mutex_idx:literal
					),*
					$(,)?
				}
			)?

		}
	)*

) {
	paste::paste! {
		$(
			/// Locker inner
			#[derive(Debug)]
			pub struct [< $LockerName Inner >] {
				// Async mutexes
				$(
					$(
						$async_mutex_name: Arc<Mutex<$async_mutex_ty>>,
					)*
				)?
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
				pub fn $new(
					// Async mutexes
					$( $( $async_mutex_name: Arc<Mutex<$async_mutex_ty>> ),* )?
				) -> Self {
					// TODO: Don't leak and instead drop it only when dropping when `STATE == 0`.
					let inner = [< $LockerName Inner >] {
						// Async mutexes
						$( $( $async_mutex_name, )* )?
					};
					let inner = Box::leak(Box::new(inner));

					Self { $inner: inner }
				}

				/// Locks the resource `R`
				#[track_caller]
				pub async fn $lock_async_mutex<'locker, R>(
					&'locker mut self,
				) -> (MutexGuard<'locker, R>, <Self as AsyncMutexLocker<R>>::Next<'locker>)
				where
					Self: AsyncMutexLocker<R>,
					R: 'locker,
				{
					self.lock_resource().await
				}
			}

			// Async mutexes
			$(
				#[duplicate::duplicate_item(
					ResourceTy field NEXT_STATE;
					$(
						[$async_mutex_ty] [$async_mutex_name] [{ $async_mutex_idx + 1 }];
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
			)?
		)*
	}
}
