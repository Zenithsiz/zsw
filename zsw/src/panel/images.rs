//! Panel images

// Modules
mod image;

// Exports
pub use self::image::PanelImage;

// Imports
use {
	super::{PanelGeometry, PanelsRendererLayouts, PlaylistPlayer},
	crate::image_loader::{ImageReceiver, ImageRequest, ImageRequester},
	futures::lock::Mutex,
	std::{self, mem},
	zsw_wgpu::WgpuShared,
};

/// Panel images
#[derive(Debug)]
pub struct PanelImages {
	/// Previous image
	prev: PanelImage,

	/// Current image
	cur: PanelImage,

	/// Next image
	next: PanelImage,

	/// Playlist player
	playlist_player: Mutex<PlaylistPlayer>,

	/// Texture sampler
	texture_sampler: wgpu::Sampler,

	/// Texture bind group
	image_bind_group: wgpu::BindGroup,

	/// Scheduled image receiver.
	scheduled_image_receiver: Option<ImageReceiver>,
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
			texture_sampler,
			image_bind_group,
			scheduled_image_receiver: None,
			playlist_player: Mutex::new(playlist_player),
		}
	}

	/// Returns the image bind group for these images
	pub fn image_bind_group(&self) -> &wgpu::BindGroup {
		&self.image_bind_group
	}

	/// Returns the playlist player for these images
	pub fn playlist_player(&self) -> &Mutex<PlaylistPlayer> {
		&self.playlist_player
	}

	/// Steps to the previous image, if any
	pub async fn step_prev(
		&mut self,
		wgpu_shared: &WgpuShared,
		renderer_layouts: &PanelsRendererLayouts,
	) -> Result<(), ()> {
		self.playlist_player.lock().await.step_prev()?;
		mem::swap(&mut self.cur, &mut self.next);
		mem::swap(&mut self.prev, &mut self.cur);
		self.prev = PanelImage::empty();
		self.update_image_bind_group(wgpu_shared, renderer_layouts);
		Ok(())
	}

	/// Steps to the next image
	pub async fn step_next(&mut self, wgpu_shared: &WgpuShared, renderer_layouts: &PanelsRendererLayouts) {
		mem::swap(&mut self.prev, &mut self.cur);
		mem::swap(&mut self.cur, &mut self.next);
		self.next = PanelImage::empty();
		self.playlist_player.lock().await.step_next();
		self.update_image_bind_group(wgpu_shared, renderer_layouts);
	}

	/// Loads any missing images, prioritizing the current, then next, then previous.
	///
	/// Requests images if missing any.
	pub async fn load_missing(
		&mut self,
		wgpu_shared: &WgpuShared,
		renderer_layouts: &PanelsRendererLayouts,
		image_requester: &ImageRequester,
		geometries: &[PanelGeometry],
	) {
		// Get the image receiver, or schedule it.
		let Some(image_receiver) = self.scheduled_image_receiver.as_mut() else {
			_ = self.schedule_load_image(wgpu_shared, image_requester, geometries).await;
			return;
		};

		// Then try to get the response
		let Some(response) = image_receiver.try_recv() else {
			return;
		};

		// Remove the exhausted receiver
		self.scheduled_image_receiver = None;

		// Then check if we got the image
		let image = match response.image_res {
			// If so, return it
			Ok(image) => image,

			// Else, log an error, remove the image and re-schedule it
			Err(err) => {
				{
					tracing::warn!(image_path = ?response.request.path, ?err, "Unable to load image, removing it from player");
					let mut playlist_player = self.playlist_player.lock().await;
					playlist_player.remove(&response.request.path);
				}

				_ = self.schedule_load_image(wgpu_shared, image_requester, geometries).await;
				return;
			},
		};

		// Get which slot to load the image into
		let slot = {
			let playlist_player = self.playlist_player.lock().await;
			match response.request.playlist_pos {
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
				Slot::Prev => self.prev = PanelImage::new(wgpu_shared, image),
				Slot::Cur => self.cur = PanelImage::new(wgpu_shared, image),
				Slot::Next => self.next = PanelImage::new(wgpu_shared, image),
			}
			self.update_image_bind_group(wgpu_shared, renderer_layouts);
		}
	}

	/// Schedules a new image.
	///
	/// Returns the position in the playlist we're loading
	///
	/// If the playlist player is empty, does not schedule.
	/// If already scheduled, returns
	async fn schedule_load_image(
		&mut self,
		wgpu_shared: &WgpuShared,
		image_requester: &ImageRequester,
		geometries: &[PanelGeometry],
	) -> Option<usize> {
		if self.scheduled_image_receiver.is_some() {
			return None;
		}

		// Get the playlist position and path to load
		let mut playlist_player = self.playlist_player.lock().await;
		let (playlist_pos, image_path) = match () {
			() if !self.cur.is_loaded() => (playlist_player.cur_pos(), playlist_player.cur()?),
			() if !self.next.is_loaded() => (playlist_player.next_pos(), playlist_player.next()?),
			() if !self.prev.is_loaded() => (playlist_player.prev_pos()?, playlist_player.prev()?),
			() => return None,
		};

		let wgpu_limits = wgpu_shared.device.limits();
		self.scheduled_image_receiver = Some(image_requester.request(ImageRequest {
			path: image_path.to_path_buf(),
			sizes: geometries.iter().map(|geometry| geometry.geometry.size).collect(),
			max_image_size: wgpu_limits.max_texture_dimension_2d,
			playlist_pos,
		}));

		Some(playlist_pos)
	}

	/// Updates the image bind group
	fn update_image_bind_group(&mut self, wgpu_shared: &WgpuShared, renderer_layouts: &PanelsRendererLayouts) {
		self.image_bind_group = self::create_image_bind_group(
			wgpu_shared,
			&renderer_layouts.image_bind_group_layout,
			self.prev.texture_view(wgpu_shared),
			self.cur.texture_view(wgpu_shared),
			self.next.texture_view(wgpu_shared),
			&self.texture_sampler,
		);
	}

	/// Returns the previous image
	pub fn prev(&self) -> &PanelImage {
		&self.prev
	}

	/// Returns the previous image mutably
	pub fn prev_mut(&mut self) -> &mut PanelImage {
		&mut self.prev
	}

	/// Returns the current image
	pub fn cur(&self) -> &PanelImage {
		&self.cur
	}

	/// Returns the current image mutably
	pub fn cur_mut(&mut self) -> &mut PanelImage {
		&mut self.cur
	}

	/// Returns the next image
	pub fn next(&self) -> &PanelImage {
		&self.next
	}

	/// Returns the next image mutably
	pub fn next_mut(&mut self) -> &mut PanelImage {
		&mut self.next
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
