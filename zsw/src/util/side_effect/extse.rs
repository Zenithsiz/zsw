//! External crate wrappers with side effects
//!
//! `EXTern Side Effect`

// Imports
use super::{MightDeadlock, WithSideEffect};

/// Side effect wrapper for [`crossbeam::channel::Receiver`]
pub trait CrossBeamChannelReceiverSE<T> {
	fn recv_se(&self) -> WithSideEffect<Result<T, crossbeam::channel::RecvError>, MightDeadlock>;
}

impl<T> CrossBeamChannelReceiverSE<T> for crossbeam::channel::Receiver<T> {
	fn recv_se(&self) -> WithSideEffect<Result<T, crossbeam::channel::RecvError>, MightDeadlock> {
		#[allow(clippy::disallowed_methods)] // We're wrapping it
		WithSideEffect::new(self.recv())
	}
}
