//! Image loader

// Imports
use {image::DynamicImage, std::path::PathBuf, tokio::sync::oneshot, zsw_util::Rect};

/// Request
pub struct Request {
	/// Path
	pub path: PathBuf,

	/// Geometries
	///
	/// Image must fit within these geometries
	pub geometries: Vec<Rect<i32, u32>>,
}

/// Response
pub struct Response {
	/// Loaded image
	pub image: Result<DynamicImage, anyhow::Error>,
}


/// Image requester
pub struct ImageRequester {
	/// Request sender
	req_tx: async_channel::Sender<(Request, oneshot::Sender<Response>)>,
}

impl ImageRequester {
	/// Sends a request
	pub fn request(&self, request: Request) -> ResponseReceiver {
		let (ret_tx, ret_rx) = oneshot::channel();
		match self.req_tx.try_send((request, ret_tx)) {
			Ok(()) => (),
			Err(async_channel::TrySendError::Closed(_)) => unreachable!("Unbounded channel was full"),
			Err(async_channel::TrySendError::Full(_)) => panic!("Image loader quit"),
		}

		ResponseReceiver { ret_rx }
	}
}

/// Response receiver
pub struct ResponseReceiver {
	/// Return receiver
	pub(super) ret_rx: oneshot::Receiver<Response>,
}

impl ResponseReceiver {
	/// Tries to receive the response
	pub fn try_recv(&mut self) -> Option<Response> {
		match self.ret_rx.try_recv() {
			Ok(response) => Some(response),
			Err(oneshot::error::TryRecvError::Empty) => None,
			Err(oneshot::error::TryRecvError::Closed) => panic!("Image loader dropped request"),
		}
	}
}

/// Image loader
pub struct ImageLoader {
	/// Request receiver
	req_rx: async_channel::Receiver<(Request, oneshot::Sender<Response>)>,
}

/// Creates the image loader service
pub fn create() -> (ImageLoader, ImageRequester) {
	let (req_tx, req_rx) = async_channel::unbounded();
	(ImageLoader { req_rx }, ImageRequester { req_tx })
}
