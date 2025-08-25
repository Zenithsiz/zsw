//! Image loader

// Imports
use {
	app_error::Context,
	futures::StreamExt,
	image::DynamicImage,
	std::path::PathBuf,
	tokio::sync::oneshot,
	tracing::Instrument,
	zsw_util::AppError,
	zutil_cloned::cloned,
};

/// Image
#[derive(Debug)]
pub struct Image {
	/// Path
	pub path: PathBuf,

	/// Image
	pub image: DynamicImage,
}

/// Request
#[derive(Debug)]
pub struct ImageRequest {
	/// Path
	pub path: PathBuf,

	/// Max image size
	pub max_image_size: u32,

	/// Playlist position for this image
	pub playlist_pos: usize,
}

/// Response
#[derive(Debug)]
pub struct ImageResponse {
	/// Request
	pub request: ImageRequest,

	/// Image result
	pub image_res: Result<Image, AppError>,
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
	// TODO: Receive this as an argument instead?
	req_rx: async_channel::Receiver<(ImageRequest, oneshot::Sender<ImageResponse>)>,
}

impl ImageLoader {
	/// Runs the image loader.
	pub async fn run(self) -> Result<(), AppError> {
		// Accept all requests in parallel
		self.req_rx
			.then(|(request, response_tx)| async {
				// Load the image, then send it
				let image_res = Self::load(&request).await;
				if let Err(err) = &image_res {
					tracing::warn!("Unable to load image {:?}: {}", request.path, err.pretty());
				}

				if let Err(response) = response_tx.send(ImageResponse { request, image_res }) {
					tracing::warn!("Request for image {:?} was aborted", response.request.path);
				}
			})
			.collect::<()>()
			.await;

		Ok(())
	}

	/// Loads an image by request
	async fn load(request: &ImageRequest) -> Result<Image, AppError> {
		// Load the image
		tracing::trace!("Loading image {:?}", request.path);
		#[cloned(image_path = request.path)]
		let mut image = tokio::task::spawn_blocking(move || image::open(image_path))
			.instrument(tracing::trace_span!("Loading image"))
			.await
			.context("Unable to join image load task")?
			.context("Unable to open image")?;
		tracing::trace!("Loaded image {:?} ({}x{})", request.path, image.width(), image.height());

		// If the image is too big, resize it
		if image.width() >= request.max_image_size || image.height() >= request.max_image_size {
			let max_image_size = request.max_image_size;

			tracing::trace!(
				"Resizing image {:?} ({}x{}) to at most {max_image_size}x{max_image_size}",
				request.path,
				image.width(),
				image.height()
			);
			image = tokio::task::spawn_blocking(move || {
				image.resize(max_image_size, max_image_size, image::imageops::FilterType::Nearest)
			})
			.instrument(tracing::trace_span!("Resizing image"))
			.await
			.context("Failed to join image resize task")?;
			tracing::trace!(
				"Resized image {:?} to {}x{}",
				request.path,
				image.width(),
				image.height()
			);
		}

		Ok(Image {
			path: request.path.clone(),
			image,
		})
	}
}

/// Creates the image loader service
pub async fn create() -> Result<(ImageLoader, ImageRequester), AppError> {
	let (req_tx, req_rx) = async_channel::unbounded();
	Ok((ImageLoader { req_rx }, ImageRequester { req_tx }))
}
