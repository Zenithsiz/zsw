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
	fn as_inner(&self) -> &meetup::Sender<Self::Inner>;
}

/// Meetup sender resource extension trait
#[extend::ext(name = MeetupSenderResourceExt)]
#[sealed::sealed]
pub impl<R: MeetupSenderResource> R {
	/// Sends the resource `R` to this meetup channel
	#[track_caller]
	async fn send<'locker, const STATE: usize>(&'locker self, _locker: &'locker mut Locker<STATE>, resource: R::Inner)
	where
		Locker<STATE>: MeetupSenderLocker<R>,
	{
		self.as_inner().send(resource).await;
	}
}

/// Locker for meetup channels
// Note: No `NEXT_STATE`, as we don't keep anything locked.
#[sealed::sealed(pub(super))]
pub trait MeetupSenderLocker<R> {}

/// Creates a meetup resource type
pub macro resource_impl(
	$Name:ident($Inner:ty);
	fn $new:ident(...) -> ...;

	states {
		$( $CUR_STATE:literal ),* $(,)?
	}
) {
	#[derive(Debug)]
	pub struct $Name(meetup::Sender<$Inner>);

	impl $Name {
		/// Creates the rwlock
		// TODO: Not receive a built sender and instead create a `(Sender, Receiver)` pair?
		pub fn $new(inner: meetup::Sender<$Inner>) -> Self {
			Self(inner)
		}
	}

	#[sealed::sealed]
	impl MeetupSenderResource for $Name {
		type Inner = $Inner;

		fn as_inner(&self) -> &meetup::Sender<Self::Inner> {
			&self.0
		}
	}

	$(
		#[sealed::sealed]
		impl MeetupSenderLocker<$Name> for Locker<$CUR_STATE> {}
	)*
}
