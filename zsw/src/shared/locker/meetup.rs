//! Meetup channel locking.
//!
//! Uses the `meetup` channel.

// Imports
use {super::Locker, zsw_util::meetup};

/// Meetup sender resource
#[sealed::sealed(pub(super))]
pub trait MeetupSenderResource {
	/// Inner type
	type Inner;

	/// Returns the inner meetup sender
	#[doc(hidden)]
	fn as_inner(&self) -> &meetup::Sender<Self::Inner>;

	/// Sends the resource `R` to this meetup channel
	#[track_caller]
	async fn send<'locker, 'prev_locker, const STATE: usize>(
		&'locker self,
		locker: &'locker mut Locker<'prev_locker, STATE>,
		resource: Self::Inner,
	) where
		Self: Sized,
		Locker<'prev_locker, STATE>: MeetupSenderLocker<Self>,
	{
		locker.ensure_same_task();
		self.as_inner().send(resource).await;
	}
}

/// Locker for meetup channels
// Note: No `NEXT_STATE`, as we don't keep anything locked.
#[sealed::sealed(pub(super))]
pub trait MeetupSenderLocker<R> {}

/// Creates a meetup resource type
pub macro resource_impl(
	$Name:ident { $field:ident: $Inner:ty };
	fn $new:ident(...) -> ...;

	states {
		$( $CUR_STATE:literal ),* $(,)?
	}
) {
	#[derive(Debug)]
	pub struct $Name {
		$field: meetup::Sender<$Inner>
	}

	impl $Name {
		/// Creates the rwlock
		// TODO: Not receive a built sender and instead create a `(Sender, Receiver)` pair?
		pub fn $new(inner: meetup::Sender<$Inner>) -> Self {
			Self { $field: inner }
		}
	}

	#[sealed::sealed]
	impl MeetupSenderResource for $Name {
		type Inner = $Inner;

		fn as_inner(&self) -> &meetup::Sender<Self::Inner> {
			&self.$field
		}
	}

	$(
		#[sealed::sealed]
		impl<'locker> MeetupSenderLocker<$Name> for Locker<'locker, $CUR_STATE> {}
	)*
}