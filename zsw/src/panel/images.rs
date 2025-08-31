//! Panel images

// Modules
mod image;

// Exports
pub use self::image::PanelImage;

// Imports
use {
	super::PlaylistPlayer,
	::image::DynamicImage,
	app_error::Context,
	core::task::Poll,
	futures::FutureExt,
	std::{self, mem, path::Path, sync::Arc},
	tracing::Instrument,
	zsw_util::AppError,
	zsw_wgpu::WgpuShared,
	zutil_cloned::cloned,
};

/// Panel images
#[derive(Debug)]
pub struct PanelImages {
	/// Previous image
	pub prev: PanelImage,

	/// Current image
	pub cur: PanelImage,

	/// Next image
	pub next: PanelImage,

	/// Texture sampler
	pub image_sampler: Option<wgpu::Sampler>,

	/// Texture bind group
	pub image_bind_group: Option<wgpu::BindGroup>,

	/// Image loading task
	pub image_load_task: Option<tokio::task::JoinHandle<ImageLoadRes>>,
}

impl PanelImages {
	/// Creates a new panel
	#[must_use]
	pub fn new() -> Self {
		Self {
			prev:             PanelImage::empty(),
			cur:              PanelImage::empty(),
			next:             PanelImage::empty(),
			image_sampler:    None,
			image_bind_group: None,
			image_load_task:  None,
		}
	}

	/// Steps to the previous image, if any
	///
	/// If successfull, starts loading any missing images
	///
	/// Returns `Err(())` if this would erase the current image.
	pub fn step_prev(&mut self, playlist_player: &mut PlaylistPlayer, wgpu_shared: &WgpuShared) -> Result<(), ()> {
		playlist_player.step_prev()?;
		mem::swap(&mut self.cur, &mut self.next);
		mem::swap(&mut self.prev, &mut self.cur);
		self.prev = PanelImage::empty();
		self.image_bind_group = None;
		self.load_missing(playlist_player, wgpu_shared);

		Ok(())
	}

	/// Steps to the next image.
	///
	/// If successfull, starts loading any missing images
	///
	/// Returns `Err(())` if this would erase the current image.
	pub fn step_next(&mut self, playlist_player: &mut PlaylistPlayer, wgpu_shared: &WgpuShared) -> Result<(), ()> {
		if !self.next.is_loaded() {
			return Err(());
		}

		playlist_player.step_next();
		mem::swap(&mut self.prev, &mut self.cur);
		mem::swap(&mut self.cur, &mut self.next);
		self.next = PanelImage::empty();
		self.image_bind_group = None;
		self.load_missing(playlist_player, wgpu_shared);

		Ok(())
	}

	/// Loads any missing images, prioritizing the current, then next, then previous.
	///
	/// Requests images if missing any.
	pub fn load_missing(&mut self, playlist_player: &mut PlaylistPlayer, wgpu_shared: &WgpuShared) {
		// Get the next image, if we can
		let Some(res) = self.next_image(playlist_player, wgpu_shared) else {
			return;
		};

		// Then check if we got the image
		let image = match res.image_res {
			// If so, return it
			Ok(image) => image,

			// Else, log an error, remove the image and re-schedule it
			Err(err) => {
				tracing::warn!(
					"Unable to load image {:?}, removing it from player: {}",
					res.path,
					err.pretty()
				);
				playlist_player.remove(&res.path);

				_ = self.schedule_load_image(playlist_player, wgpu_shared);
				return;
			},
		};

		// Get which slot to load the image into
		let slot = {
			match res.playlist_pos {
				pos if Some(pos) == playlist_player.prev_pos() => Some(Slot::Prev),
				pos if pos == playlist_player.cur_pos() => Some(Slot::Cur),
				pos if pos == playlist_player.next_pos() => Some(Slot::Next),
				pos => {
					tracing::warn!(
						pos,
						playlist_pos = playlist_player.cur_pos(),
						"Discarding loaded image due to position being too far",
					);
					None
				},
			}
		};

		if let Some(slot) = slot {
			match slot {
				Slot::Prev => self.prev = PanelImage::new(wgpu_shared, res.path, image),
				Slot::Cur => self.cur = PanelImage::new(wgpu_shared, res.path, image),
				Slot::Next => self.next = PanelImage::new(wgpu_shared, res.path, image),
			}
			self.image_bind_group = None;
		}
	}

	/// Gets the next image, if any.
	///
	/// If an image is not scheduled, schedules it, even after
	/// successfully returning an image
	fn next_image(&mut self, playlist_player: &mut PlaylistPlayer, wgpu_shared: &WgpuShared) -> Option<ImageLoadRes> {
		// Get the load task
		let task = self.schedule_load_image(playlist_player, wgpu_shared)?;

		// Then try to get the response
		// Note: If we did it, immediately invalidate the exhausted future
		//       and schedule a new image to be loaded
		let Poll::Ready(res) = task.poll_unpin(&mut std::task::Context::from_waker(std::task::Waker::noop())) else {
			return None;
		};
		self.image_load_task = None;
		_ = self.schedule_load_image(playlist_player, wgpu_shared);

		// Finally make sure the task didn't get cancelled
		let res = match res {
			Ok(res) => res,
			Err(err) => {
				let err = AppError::new(&err);
				tracing::warn!("Scheduled image loader was cancelled: {}", err.pretty());
				return None;
			},
		};

		Some(res)
	}

	/// Schedules a new image.
	///
	/// Returns the handle to the load task
	fn schedule_load_image(
		&mut self,
		playlist_player: &mut PlaylistPlayer,
		wgpu_shared: &WgpuShared,
	) -> Option<&mut tokio::task::JoinHandle<ImageLoadRes>> {
		match self.image_load_task {
			// TODO: This is required because of the borrow checker
			Some(_) => Some(self.image_load_task.as_mut().expect("Just checked")),
			None => {
				// Get the playlist position and path to load
				let (playlist_pos, path) = match () {
					() if !self.cur.is_loaded() => (playlist_player.cur_pos(), playlist_player.cur()?),
					() if !self.next.is_loaded() => (playlist_player.next_pos(), playlist_player.next()?),
					() if !self.prev.is_loaded() => (playlist_player.prev_pos()?, playlist_player.prev()?),
					() => return None,
				};

				let wgpu_limits = wgpu_shared.device.limits();
				let handle = self.image_load_task.insert(tokio::task::spawn(async move {
					let image_res = self::load(&path, wgpu_limits.max_texture_dimension_2d).await;
					ImageLoadRes {
						path,
						playlist_pos,
						image_res,
					}
				}));

				Some(handle)
			},
		}
	}

	/// Returns if all images are empty
	pub fn is_empty(&self) -> bool {
		matches!(self.prev, PanelImage::Empty) &&
			matches!(self.cur, PanelImage::Empty) &&
			matches!(self.next, PanelImage::Empty)
	}
}

/// Image slot
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum Slot {
	Prev,
	Cur,
	Next,
}

#[derive(Debug)]
pub struct ImageLoadRes {
	path:         Arc<Path>,
	playlist_pos: usize,
	image_res:    Result<DynamicImage, AppError>,
}

/// Loads an image
pub async fn load(path: &Arc<Path>, max_image_size: u32) -> Result<DynamicImage, AppError> {
	// Load the image
	tracing::trace!("Loading image {:?}", path);
	#[cloned(path)]
	let mut image = tokio::task::spawn_blocking(move || ::image::open(path))
		.instrument(tracing::trace_span!("Loading image"))
		.await
		.context("Unable to join image load task")?
		.context("Unable to open image")?;
	tracing::trace!("Loaded image {:?} ({}x{})", path, image.width(), image.height());

	// If the image is too big, resize it
	if image.width() >= max_image_size || image.height() >= max_image_size {
		tracing::trace!(
			"Resizing image {:?} ({}x{}) to at most {max_image_size}x{max_image_size}",
			path,
			image.width(),
			image.height()
		);
		image = tokio::task::spawn_blocking(move || {
			image.resize(max_image_size, max_image_size, ::image::imageops::FilterType::Nearest)
		})
		.instrument(tracing::trace_span!("Resizing image"))
		.await
		.context("Failed to join image resize task")?;
		tracing::trace!("Resized image {:?} to {}x{}", path, image.width(), image.height());
	}

	Ok(image)
}
