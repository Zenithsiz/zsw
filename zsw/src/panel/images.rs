//! Panel images

// Modules
mod image;

// Exports
pub use self::image::PanelFadeImage;

// Imports
use {
	super::PlaylistPlayer,
	::image::DynamicImage,
	app_error::Context,
	std::{self, mem, path::Path, sync::Arc},
	tracing::Instrument,
	zsw_util::{AppError, Loadable, loadable::Loader},
	zsw_wgpu::Wgpu,
	zutil_cloned::cloned,
};

/// Panel fade images
#[derive(Debug)]
pub struct PanelFadeImages {
	/// Previous image
	pub prev: PanelFadeImage,

	/// Current image
	pub cur: PanelFadeImage,

	/// Next image
	pub next: PanelFadeImage,

	/// Texture sampler
	pub image_sampler: Option<wgpu::Sampler>,

	/// Texture bind group
	pub image_bind_group: Option<wgpu::BindGroup>,

	/// Next image
	pub next_image: Loadable<ImageLoadRes, NextImageLoader>,
}

/// Arguments to the next image loader
pub struct NextImageArgs {
	playlist_pos:   usize,
	path:           Arc<Path>,
	max_image_size: u32,
}

pub type NextImageLoader = impl Loader<(NextImageArgs,), ImageLoadRes>;

impl PanelFadeImages {
	/// Creates a new panel
	#[must_use]
	#[define_opaque(NextImageLoader)]
	pub fn new() -> Self {
		Self {
			prev:             PanelFadeImage::empty(),
			cur:              PanelFadeImage::empty(),
			next:             PanelFadeImage::empty(),
			image_sampler:    None,
			image_bind_group: None,
			next_image:       Loadable::new(async move |args: NextImageArgs| {
				let image_res = self::load(&args.path, args.max_image_size).await;
				ImageLoadRes {
					path: args.path,
					playlist_pos: args.playlist_pos,
					image_res,
				}
			}),
		}
	}

	/// Steps to the previous image, if any
	///
	/// If successful, starts loading any missing images
	///
	/// Returns `Err(())` if this would erase the current image.
	pub fn step_prev(&mut self, playlist_player: &mut PlaylistPlayer, wgpu: &Wgpu) -> Result<(), ()> {
		playlist_player.step_prev()?;
		mem::swap(&mut self.cur, &mut self.next);
		mem::swap(&mut self.prev, &mut self.cur);
		self.prev = PanelFadeImage::empty();
		self.image_bind_group = None;
		self.load_missing(playlist_player, wgpu);

		Ok(())
	}

	/// Steps to the next image.
	///
	/// If successful, starts loading any missing images
	///
	/// Returns `Err(())` if this would erase the current image.
	pub fn step_next(&mut self, playlist_player: &mut PlaylistPlayer, wgpu: &Wgpu) -> Result<(), ()> {
		if !self.next.is_loaded() {
			return Err(());
		}

		playlist_player.step_next();
		mem::swap(&mut self.prev, &mut self.cur);
		mem::swap(&mut self.cur, &mut self.next);
		self.next = PanelFadeImage::empty();
		self.image_bind_group = None;
		self.load_missing(playlist_player, wgpu);

		Ok(())
	}

	/// Loads any missing images, prioritizing the current, then next, then previous.
	///
	/// Requests images if missing any.
	pub fn load_missing(&mut self, playlist_player: &mut PlaylistPlayer, wgpu: &Wgpu) {
		// Get the next image, if we can
		let Some(res) = self.next_image(playlist_player, wgpu) else {
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

				_ = self.schedule_load_image(playlist_player, wgpu);
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
				Slot::Prev => self.prev = PanelFadeImage::new(wgpu, res.path, image),
				Slot::Cur => self.cur = PanelFadeImage::new(wgpu, res.path, image),
				Slot::Next => self.next = PanelFadeImage::new(wgpu, res.path, image),
			}
			self.image_bind_group = None;
		}
	}

	/// Gets the next image, if any.
	///
	/// If an image is not scheduled, schedules it, even after
	/// successfully returning an image
	fn next_image(&mut self, playlist_player: &mut PlaylistPlayer, wgpu: &Wgpu) -> Option<ImageLoadRes> {
		// Schedule it and try to take any existing image result
		_ = self.schedule_load_image(playlist_player, wgpu);
		self.next_image.take()
	}

	/// Schedules a new image.
	///
	/// If the image is loaded, returns it
	fn schedule_load_image(&mut self, playlist_player: &mut PlaylistPlayer, wgpu: &Wgpu) -> Option<&mut ImageLoadRes> {
		// If we're loaded, just return it
		// Note: We can't use if-let due to a borrow-checker limitation
		if self.next_image.get().is_some() {
			return self.next_image.get_mut();
		}

		// Get the playlist position and path to load
		let (playlist_pos, path) = match () {
			() if !self.cur.is_loaded() => (playlist_player.cur_pos(), playlist_player.cur()?),
			() if !self.next.is_loaded() => (playlist_player.next_pos(), playlist_player.next()?),
			() if !self.prev.is_loaded() => (playlist_player.prev_pos()?, playlist_player.prev()?),
			() => return None,
		};

		let max_image_size = wgpu.device.limits().max_texture_dimension_2d;

		self.next_image.try_load((NextImageArgs {
			playlist_pos,
			path,
			max_image_size,
		},))
	}

	/// Returns if all images are empty
	pub fn is_empty(&self) -> bool {
		matches!(self.prev, PanelFadeImage::Empty) &&
			matches!(self.cur, PanelFadeImage::Empty) &&
			matches!(self.next, PanelFadeImage::Empty)
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
