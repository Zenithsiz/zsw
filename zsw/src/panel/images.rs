//! Panel images

// Modules
mod image;

// Exports
pub use self::image::PanelImage;

// Imports
use {
	super::{PanelsRendererLayouts, PlaylistPlayer},
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

	/// Playlist player
	pub playlist_player: PlaylistPlayer,

	/// Texture sampler
	pub texture_sampler: wgpu::Sampler,

	/// Texture bind group
	pub image_bind_group: wgpu::BindGroup,

	/// Image loading task
	pub image_load_task: Option<tokio::task::JoinHandle<ImageLoadRes>>,
}

impl PanelImages {
	/// Creates a new panel
	#[must_use]
	pub fn new(
		playlist_player: PlaylistPlayer,
		wgpu_shared: &WgpuShared,
		renderer_layouts: &PanelsRendererLayouts,
	) -> Self {
		// Create the textures
		let image_prev = PanelImage::empty();
		let image_cur = PanelImage::empty();
		let image_next = PanelImage::empty();
		let texture_sampler = self::create_texture_sampler(wgpu_shared);
		let image_bind_group = self::create_image_bind_group(
			wgpu_shared,
			&renderer_layouts.image_bind_group_layout,
			image_prev.texture_view(wgpu_shared),
			image_cur.texture_view(wgpu_shared),
			image_next.texture_view(wgpu_shared),
			&texture_sampler,
		);

		Self {
			prev: image_prev,
			cur: image_cur,
			next: image_next,
			playlist_player,
			texture_sampler,
			image_bind_group,
			image_load_task: None,
		}
	}

	/// Steps to the previous image, if any
	///
	/// Returns `Err(())` if this would erase the current image.
	pub fn step_prev(&mut self, wgpu_shared: &WgpuShared, renderer_layouts: &PanelsRendererLayouts) -> Result<(), ()> {
		self.playlist_player.step_prev()?;
		mem::swap(&mut self.cur, &mut self.next);
		mem::swap(&mut self.prev, &mut self.cur);
		self.prev = PanelImage::empty();
		self.update_image_bind_group(wgpu_shared, renderer_layouts);
		Ok(())
	}

	/// Steps to the next image.
	///
	/// Returns `Err(())` if this would erase the current image.
	pub fn step_next(&mut self, wgpu_shared: &WgpuShared, renderer_layouts: &PanelsRendererLayouts) -> Result<(), ()> {
		if !self.next.is_loaded() {
			return Err(());
		}

		self.playlist_player.step_next();
		mem::swap(&mut self.prev, &mut self.cur);
		mem::swap(&mut self.cur, &mut self.next);
		self.next = PanelImage::empty();
		self.update_image_bind_group(wgpu_shared, renderer_layouts);

		Ok(())
	}

	/// Loads any missing images, prioritizing the current, then next, then previous.
	///
	/// Requests images if missing any.
	pub fn load_missing(&mut self, wgpu_shared: &WgpuShared, renderer_layouts: &PanelsRendererLayouts) {
		// Get the next image, if we can
		let Some(res) = self.next_image(wgpu_shared) else {
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
				self.playlist_player.remove(&res.path);

				_ = self.schedule_load_image(wgpu_shared);
				return;
			},
		};

		// Get which slot to load the image into
		let slot = {
			match res.playlist_pos {
				pos if Some(pos) == self.playlist_player.prev_pos() => Some(Slot::Prev),
				pos if pos == self.playlist_player.cur_pos() => Some(Slot::Cur),
				pos if pos == self.playlist_player.next_pos() => Some(Slot::Next),
				pos => {
					tracing::warn!(
						pos,
						playlist_pos = self.playlist_player.cur_pos(),
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
			self.update_image_bind_group(wgpu_shared, renderer_layouts);
		}
	}

	/// Gets the next image, if any.
	///
	/// If an image is not scheduled, schedules it, even after
	/// successfully returning an image
	fn next_image(&mut self, wgpu_shared: &WgpuShared) -> Option<ImageLoadRes> {
		// Get the load task
		let task = self.schedule_load_image(wgpu_shared)?;

		// Then try to get the response
		// Note: If we did it, immediately invalidate the exhausted future
		//       and schedule a new image to be loaded
		let Poll::Ready(res) = task.poll_unpin(&mut std::task::Context::from_waker(std::task::Waker::noop())) else {
			return None;
		};
		self.image_load_task = None;
		_ = self.schedule_load_image(wgpu_shared);

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
	fn schedule_load_image(&mut self, wgpu_shared: &WgpuShared) -> Option<&mut tokio::task::JoinHandle<ImageLoadRes>> {
		match self.image_load_task {
			// TODO: This is required because of the borrow checker
			Some(_) => Some(self.image_load_task.as_mut().expect("Just checked")),
			None => {
				// Get the playlist position and path to load
				let (playlist_pos, path) = match () {
					() if !self.cur.is_loaded() => (self.playlist_player.cur_pos(), self.playlist_player.cur()?),
					() if !self.next.is_loaded() => (self.playlist_player.next_pos(), self.playlist_player.next()?),
					() if !self.prev.is_loaded() => (self.playlist_player.prev_pos()?, self.playlist_player.prev()?),
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

	/// Updates the image bind group
	pub fn update_image_bind_group(&mut self, wgpu_shared: &WgpuShared, renderer_layouts: &PanelsRendererLayouts) {
		self.image_bind_group = self::create_image_bind_group(
			wgpu_shared,
			&renderer_layouts.image_bind_group_layout,
			self.prev.texture_view(wgpu_shared),
			self.cur.texture_view(wgpu_shared),
			self.next.texture_view(wgpu_shared),
			&self.texture_sampler,
		);
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

/// Creates the texture sampler
fn create_texture_sampler(wgpu_shared: &WgpuShared) -> wgpu::Sampler {
	let descriptor = wgpu::SamplerDescriptor {
		label: Some("[zsw::panel] Texture sampler"),
		address_mode_u: wgpu::AddressMode::ClampToEdge,
		address_mode_v: wgpu::AddressMode::ClampToEdge,
		address_mode_w: wgpu::AddressMode::ClampToEdge,
		mag_filter: wgpu::FilterMode::Linear,
		min_filter: wgpu::FilterMode::Linear,
		mipmap_filter: wgpu::FilterMode::Linear,
		..wgpu::SamplerDescriptor::default()
	};
	wgpu_shared.device.create_sampler(&descriptor)
}

/// Creates the texture bind group
fn create_image_bind_group(
	wgpu_shared: &WgpuShared,
	bind_group_layout: &wgpu::BindGroupLayout,
	view_prev: &wgpu::TextureView,
	view_cur: &wgpu::TextureView,
	view_next: &wgpu::TextureView,
	sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
	let descriptor = wgpu::BindGroupDescriptor {
		layout:  bind_group_layout,
		entries: &[
			wgpu::BindGroupEntry {
				binding:  0,
				resource: wgpu::BindingResource::TextureView(view_prev),
			},
			wgpu::BindGroupEntry {
				binding:  1,
				resource: wgpu::BindingResource::TextureView(view_cur),
			},
			wgpu::BindGroupEntry {
				binding:  2,
				resource: wgpu::BindingResource::TextureView(view_next),
			},
			wgpu::BindGroupEntry {
				binding:  3,
				resource: wgpu::BindingResource::Sampler(sampler),
			},
		],
		label:   None,
	};
	wgpu_shared.device.create_bind_group(&descriptor)
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
