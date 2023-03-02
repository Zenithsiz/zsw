//! Image loader

// Imports
use {
	crate::panel::PanelGeometry,
	anyhow::Context,
	cgmath::Vector2,
	futures::StreamExt,
	image::DynamicImage,
	std::{
		collections::HashSet,
		path::{Path, PathBuf},
	},
	tokio::sync::{oneshot, Semaphore},
	zsw_util::Rect,
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
	pub image_res: Result<Image, anyhow::Error>,
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
	#[allow(clippy::disallowed_types)] // DEADLOCK: We never `await` it, only `try_recv`, which is non-blocking
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

	/// Upscale cache directory
	upscale_cache_dir: PathBuf,

	/// Upscale command
	upscale_cmd: Option<PathBuf>,

	/// Upscale excluded
	upscale_exclude: HashSet<PathBuf>,

	/// Upscale semaphore
	upscale_semaphore: Semaphore,
}

impl ImageLoader {
	/// Runs the image loader.
	pub async fn run(self) -> Result<(), anyhow::Error> {
		// Accept all requests in parallel
		self.req_rx
			.then(|(request, response_tx)| async {
				// Load the image, then send it
				let image_res = Self::load(
					&self.upscale_cache_dir,
					self.upscale_cmd.as_deref(),
					&self.upscale_exclude,
					&self.upscale_semaphore,
					&request,
				)
				.await;
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
	async fn load(
		upscale_cache_dir: &Path,
		upscale_cmd: Option<&Path>,
		upscale_exclude: &HashSet<PathBuf>,
		upscale_semaphore: &Semaphore,
		request: &ImageRequest,
	) -> Result<Image, anyhow::Error> {
		// Default image path
		let mut image_path = request.path.clone();

		// Check if we should upscale the image
		match Self::check_upscale(
			request,
			upscale_exclude,
			upscale_cache_dir,
			upscale_cmd,
			upscale_semaphore,
		)
		.await
		{
			Ok(Some(upscaled_image_path)) => image_path = upscaled_image_path,
			Ok(None) => (),
			Err(err) => tracing::warn!(path = ?request.path, ?err, "Unable to upscale image"),
		}


		// Load the image
		tracing::trace!(path = ?request.path, "Loading image");
		let (mut image, load_duration) =
			tokio::task::spawn_blocking(move || zsw_util::try_measure!(image::open(image_path)))
				.await
				.context("Unable to join image load task")?
				.context("Unable to open image")?;
		tracing::trace!(path = ?request.path, image_width = ?image.width(), image_height = ?image.height(), ?load_duration, "Loaded image");

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

		Ok(Image {
			path: request.path.clone(),
			image,
		})
	}

	/// Checks if an upscale is required and performs it, if so.
	///
	/// Returns the path of the upscaled image, if any
	async fn check_upscale(
		request: &ImageRequest,
		upscale_exclude: &HashSet<PathBuf>,
		upscale_cache_dir: &Path,
		upscale_cmd: Option<&Path>,
		upscale_semaphore: &Semaphore,
	) -> Result<Option<PathBuf>, anyhow::Error> {
		// Get the image size
		let (image_width, image_height) = tokio::task::spawn_blocking({
			let image_path = request.path.clone();
			move || image::image_dimensions(image_path)
		})
		.await
		.context("Unable to join image check size task")?
		.context("Unable to get image size")?;
		tracing::trace!(path = ?request.path, ?image_width, ?image_height, "Image size");

		// Then compute the minimum size required.
		// Note: If none, we don't do anything else
		let image_size = Vector2::new(image_width, image_height);
		let minimum_size = request
			.geometries
			.iter()
			.map(|geometry| Self::minimum_image_size_for_panel(Vector2::new(image_width, image_height), geometry.size))
			.reduce(|lhs, rhs| Vector2::new(lhs.x.max(rhs.x), lhs.y.max(rhs.y)));
		tracing::trace!(?request, ?minimum_size, "Minimum image size");
		let Some(minimum_size) = minimum_size else { return Ok(None); };

		// If the image isn't smaller, return
		if image_width >= minimum_size.x && image_height >= minimum_size.y {
			return Ok(None);
		}

		// If the image is in the excluded, return
		if upscale_exclude.contains(&request.path) {
			tracing::trace!(path = ?request.path, "Not upscaling due to excluded");
			return Ok(None);
		}

		// Else get the upscale command
		let Some(upscale_cmd) = upscale_cmd else {
			tracing::trace!(
				path = ?request.path,
				"Not upscaling due to no upscaler command supplied and none cached found"
			);
			return Ok(None);
		};

		// Else try to upscale
		let upscaled_image_path = Self::upscale_image(
			&request.path,
			upscale_cache_dir,
			upscale_cmd,
			upscale_semaphore,
			image_size,
			minimum_size,
		)
		.await?;

		Ok(Some(upscaled_image_path))
	}

	/// Upscales an image and returns it's path
	///
	/// Returns the path of the upscaled image, if any
	async fn upscale_image(
		image_path: &PathBuf,
		upscale_cache_dir: &Path,
		upscale_cmd: &Path,
		upscale_semaphore: &Semaphore,
		image_size: Vector2<u32>,
		minimum_size: Vector2<u32>,
	) -> Result<PathBuf, anyhow::Error> {
		// Calculate the ratio
		// Note: Needs to be a power of two for upscaler so we round it up, if necessary
		#[allow(clippy::cast_sign_loss)] // They're all positive
		let ratio = f32::max(
			minimum_size.x as f32 / image_size.x as f32,
			minimum_size.y as f32 / image_size.y as f32,
		)
		.ceil() as u32;
		let ratio = ratio.next_power_of_two();
		tracing::trace!(?image_path, ?image_size, ?minimum_size, ?ratio, "Image upscaled ratio");

		// Calculate the upscaled image path
		// TODO: Do this some other way?
		let upscaled_image_path = upscale_cache_dir.join(format!(
			"{}-x{ratio}.png",
			image_path.with_extension("").to_string_lossy().replace('/', "／")
		));

		// If the path already exists, use it
		if std::fs::try_exists(&upscaled_image_path).context("Unable to check if upscaled image exists")? {
			return Ok(upscaled_image_path);
		}

		#[allow(clippy::disallowed_methods)] // DEADLOCK: No other locks are acquired while holding the permit
		let _permit = upscale_semaphore.acquire().await;
		tracing::trace!(?image_path, ?upscaled_image_path, ?ratio, "Upscaling image");
		let ((), upscale_duration) = zsw_util::try_measure_async(async {
			tokio::process::Command::new(upscale_cmd)
				.arg("-i")
				.arg(image_path)
				.arg("-o")
				.arg(&upscaled_image_path)
				.arg("-s")
				.arg(ratio.to_string())
				.kill_on_drop(true)
				.spawn()
				.context("Unable to run upscaler")?
				.wait()
				.await
				.context("Unable to wait for upscaler to finish")?
				.exit_ok()
				.context("upscaler returned error")
		})
		.await?;
		tracing::trace!(?image_path, ?upscaled_image_path, ?upscale_duration, "Upscaled image");

		Ok(upscaled_image_path)
	}

	/// Determines the minimum size for an image for a panel
	fn minimum_image_size_for_panel(image_size: Vector2<u32>, panel_size: Vector2<u32>) -> Vector2<u32> {
		let ratio = PanelGeometry::image_ratio(panel_size, image_size);

		#[allow(clippy::cast_sign_loss)] // The sizes and ratio are positive
		Vector2::new(
			(panel_size.x as f32 / ratio.x).ceil() as u32,
			(panel_size.y as f32 / ratio.y).ceil() as u32,
		)
	}
}

/// Creates the image loader service
pub async fn create(
	upscale_cache_dir: PathBuf,
	upscale_cmd: Option<PathBuf>,
	upscale_exclude: HashSet<PathBuf>,
) -> Result<(ImageLoader, ImageRequester), anyhow::Error> {
	// Create the upscale cache directory
	tokio::fs::create_dir_all(&upscale_cache_dir)
		.await
		.context("Unable to create upscale cache directory")?;


	let (req_tx, req_rx) = async_channel::unbounded();
	Ok((
		ImageLoader {
			req_rx,
			upscale_cache_dir,
			upscale_cmd,
			upscale_exclude,
			upscale_semaphore: Semaphore::new(1),
		},
		ImageRequester { req_tx },
	))
}
