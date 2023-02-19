//! Image loader

// Imports
use {
	anyhow::Context,
	futures::StreamExt,
	image::DynamicImage,
	std::path::PathBuf,
	tokio::sync::oneshot,
	zsw_util::Rect,
};

/// Request
#[derive(Debug)]
pub struct ImageRequest {
	/// Path
	pub path: PathBuf,

	/// Geometries
	///
	/// Image must fit within these geometries
	pub geometries: Vec<Rect<i32, u32>>,

	/// Max image size
	pub max_image_size: u32,
}

/// Response
#[derive(Debug)]
pub struct ImageResponse {
	/// Request
	pub request: ImageRequest,

	/// Image result
	pub image_res: Result<DynamicImage, anyhow::Error>,
}


/// Image requester
#[derive(Debug)]
pub struct ImageRequester {
	/// Request sender
	req_tx: async_channel::Sender<(ImageRequest, oneshot::Sender<ImageResponse>)>,
}

impl ImageRequester {
	/// Sends a request
	pub fn request(&self, request: ImageRequest) -> ImageReceiver {
		let (ret_tx, ret_rx) = oneshot::channel();
		match self.req_tx.try_send((request, ret_tx)) {
			Ok(()) => (),
			Err(async_channel::TrySendError::Closed(_)) => unreachable!("Unbounded channel was full"),
			Err(async_channel::TrySendError::Full(_)) => panic!("Image loader quit"),
		}

		ImageReceiver { ret_rx }
	}
}

/// Image receiver
#[derive(Debug)]
pub struct ImageReceiver {
	/// Return receiver
	ret_rx: oneshot::Receiver<ImageResponse>,
}

impl ImageReceiver {
	/// Tries to receive the response
	pub fn try_recv(&mut self) -> Option<ImageResponse> {
		match self.ret_rx.try_recv() {
			Ok(response) => Some(response),
			Err(oneshot::error::TryRecvError::Empty) => None,
			Err(oneshot::error::TryRecvError::Closed) => panic!("Image loader dropped request"),
		}
	}
}

/// Image loader
#[derive(Debug)]
pub struct ImageLoader {
	/// Request receiver
	req_rx: async_channel::Receiver<(ImageRequest, oneshot::Sender<ImageResponse>)>,
}

impl ImageLoader {
	/// Runs the image loader.
	pub async fn run(self) -> Result<(), anyhow::Error> {
		// Accept all requests in parallel
		self.req_rx
			.then(async move |(request, response_tx)| {
				// Load the image, then send it
				let image_res = Self::load(&request).await;
				if let Err(err) = &image_res {
					tracing::warn!(?request, ?err, "Unable to load image");
				}

				if let Err(err) = response_tx.send(ImageResponse { request, image_res }) {
					tracing::warn!(?err, "Unable to response to image request");
				}
			})
			.collect::<()>()
			.await;

		Ok(())
	}

	/// Loads an image by request
	async fn load(request: &ImageRequest) -> Result<DynamicImage, anyhow::Error> {
		// Load the image
		let image_path = request.path.clone();
		tracing::trace!(path = ?request.path, "Loading image");
		let (mut image, load_duration) =
			tokio::task::spawn_blocking(move || zsw_util::try_measure!(image::open(image_path)))
				.await
				.context("Unable to join image load task")?
				.context("Unable to open image")?;
		tracing::trace!(path = ?request.path, ?load_duration, "Loaded image");

		// TODO: Use `request.geometries?` for upscaling?

		// If the image is too big, resize it
		if image.width() >= request.max_image_size || image.height() >= request.max_image_size {
			let max_image_size = request.max_image_size;
			tracing::trace!(path = ?request.path, image_width = ?image.width(), image_height = ?image.height(), ?max_image_size, "Resizing image that is too large");

			let resize_duration;
			(image, resize_duration) = tokio::task::spawn_blocking(move || {
				zsw_util::measure!(image.resize(max_image_size, max_image_size, image::imageops::FilterType::Nearest))
			})
			.await
			.context("Failed to join image resize task")?;
			tracing::trace!(path = ?request.path, image_width = ?image.width(), image_height = ?image.height(), ?resize_duration, "Resized image");
		}

		Ok(image)
	}
}

/// Creates the image loader service
pub fn create() -> (ImageLoader, ImageRequester) {
	let (req_tx, req_rx) = async_channel::unbounded();
	(ImageLoader { req_rx }, ImageRequester { req_tx })
}
