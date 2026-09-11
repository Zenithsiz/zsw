//! Fade shader

pub mod images;

pub use self::images::{Image, ImageSlot, Images};

use {
	crate::{
		menu,
		panel::{geometry, renderer::uniform},
		playlist::PlaylistPlayer,
	},
	chrono::TimeDelta,
	core::{cmp, time::Duration},
	euclid::default::{Point2D, Vector2D},
	std::sync::Arc,
	zsw_util::Rect,
	zsw_wgpu::Wgpu,
};

/// Shader
#[derive(Debug)]
pub struct Shader {
	/// Geometries
	geometries: Vec<Geometry>,

	/// If paused
	paused: bool,

	/// Kind
	kind: Kind,

	/// Current progress
	progress: Duration,

	/// Duration
	duration: Duration,

	/// Fade duration
	fade_duration: Duration,

	/// Images
	images: Images,

	/// Playlist player
	playlist_player: PlaylistPlayer,
}

impl Shader {
	pub fn new(
		geometries: Vec<Geometry>,
		duration: Duration,
		fade_duration: Duration,
		playlist_player: PlaylistPlayer,
		kind: Kind,
	) -> Self {
		Self {
			geometries,
			paused: false,
			kind,
			progress: Duration::ZERO,
			duration,
			fade_duration,
			images: Images::new(),
			playlist_player,
		}
	}

	/// Returns the bind group layouts for this shader
	#[expect(clippy::unused_self, reason = "We want to make sure the user has a shader")]
	pub fn bind_group_layouts<'a>(&self, shared: &'a Shared, wgpu: &Wgpu) -> [Option<&'a wgpu::BindGroupLayout>; 2] {
		[
			Some(shared.images.geometry_uniforms_bind_group_layout(wgpu)),
			Some(shared.images.image_bind_group_layout(wgpu)),
		]
	}

	/// Returns if any geometries in this panel intersects `rect`
	pub fn any_intersects(&self, rect: Rect<i32, u32>) -> bool {
		self.geometries.iter().any(|geometry| geometry.rect.intersects(rect))
	}

	/// Returns if any geometries in this panel contain `pos`
	pub fn any_contain(&self, pos: Point2D<i32>) -> bool {
		self.geometries.iter().any(|geometry| geometry.rect.contains(pos))
	}

	/// Sets the image progress
	pub fn set_progress(&mut self, progress: Duration) {
		self.progress = progress.clamp(self.min_progress(), self.max_progress());
	}

	/// Returns the normalized image progress
	#[must_use]
	pub fn progress_norm(&self) -> f32 {
		// Note: Image progress is linear throughout the full cycle
		self.progress.div_duration_f32(self.duration)
	}

	/// Returns the image fade duration
	pub fn fade_duration(&self) -> Duration {
		self.fade_duration
	}

	/// Sets the fade duration
	pub fn set_fade_duration(&mut self, fade_duration: Duration) {
		self.fade_duration = fade_duration.min(self.duration / 2);
		self.set_progress(self.progress);
	}

	/// Returns the fade duration normalized
	pub fn fade_duration_norm(&self) -> f32 {
		// Note: Image progress is linear throughout the full cycle
		self.fade_duration.div_duration_f32(self.duration)
	}

	/// Returns the min progress for the current image
	pub fn min_progress(&self) -> Duration {
		match self.images.prev.is_some() {
			// If we have a previous image, we can go until the very beginning
			true => Duration::ZERO,

			// Otherwise, stop before the fade
			false => self.fade_duration,
		}
	}

	/// Returns the max progress for the current image
	pub fn max_progress(&self) -> Duration {
		match (self.images.cur.is_some(), self.images.next.is_some()) {
			// If we have a next image, we can go until the full duration
			(_, true) => self.duration,

			// Otherwise, if we have a current, but no next, we can go until the fade begins
			(true, false) => self.duration.saturating_sub(self.fade_duration),

			// Finally, if we don't have any, we should stay at the beginning
			(false, false) => self.min_progress(),
		}
	}

	/// Returns the image duration
	pub fn duration(&self) -> Duration {
		self.duration
	}

	/// Sets the duration
	pub fn set_duration(&mut self, duration: Duration) {
		self.duration = duration;
		self.set_fade_duration(self.fade_duration);
	}

	/// Returns the panel kind
	pub fn kind(&self) -> Kind {
		self.kind
	}

	pub fn images(&self) -> &Images {
		&self.images
	}

	/// Returns if paused
	pub fn is_paused(&self) -> bool {
		self.paused
	}

	/// Sets this shader as paused
	pub fn set_paused(&mut self, paused: bool) {
		self.paused = paused;
	}

	/// Toggles pause of this shader
	pub fn toggle_paused(&mut self) {
		self.set_paused(!self.paused);
	}

	/// Skips to the next image.
	pub fn skip(&mut self, wgpu: &Arc<Wgpu>) {
		self.progress = match self.images.step_next(&mut self.playlist_player, wgpu) {
			Ok(()) => self.fade_duration,
			Err(()) => self.max_progress(),
		}
	}

	/// Steps this shader by a certain number of frames (potentially negative).
	pub fn step(&mut self, wgpu: &Arc<Wgpu>, delta: TimeDelta) {
		let (delta_abs, delta_is_positive) = self::time_delta_to_duration(delta);
		let next_progress = match delta_is_positive {
			true => Some(self.progress.saturating_add(delta_abs)),
			false => self.progress.checked_sub(delta_abs),
		};

		// Update the progress, potentially rolling over to the previous/next image
		self.progress = match next_progress {
			// If we have a next progress, check if we overflowed the duration
			Some(next_progress) => match next_progress.checked_sub(self.duration) {
				// If we did, `next_progress` is our progress at the next image, so try
				// to step to it.
				Some(next_progress) => match self.images.step_next(&mut self.playlist_player, wgpu) {
					// If we successfully stepped to the next image, start at the next progress
					// Note: If delta was big enough to overflow 2 durations, then cap it at the
					//       max duration of the next image.
					Ok(()) => next_progress.min(self.max_progress()),

					// Otherwise, stay at most on our max duration
					Err(()) => self.max_progress(),
				},

				// Otherwise, we're just moving within the current image, so clamp it
				// between our min and max progress
				None => next_progress.clamp(self.min_progress(), self.max_progress()),
			},

			// Otherwise, we underflowed, so try to step back
			None => match self.images.step_prev(&mut self.playlist_player, wgpu) {
				// If we successfully stepped backwards, start at where we're supposed to:
				Ok(()) => {
					// Note: This branch is only taken when `delta` is negative, so we can always
					//       subtract without checking `delta_is_positive`.
					assert!(!delta_is_positive, "Delta was negative despite having no next duration");

					// Note: If this delta actually underflowed twice, cap it at the minimum
					//       progress of the previous image instead.
					match (self.duration + self.progress).checked_sub(delta_abs) {
						Some(next_progress) => next_progress,
						None => self.min_progress(),
					}
				},

				// Otherwise, just stay at the minimum progress of the current image.
				Err(()) => self.min_progress(),
			},
		}
	}

	/// Updates this shader with a delta
	pub fn update(&mut self, wgpu: &Arc<Wgpu>, delta: Duration) {
		// Note: We always load images, even if we're paused, since the user might be
		//       moving around manually.
		self.images.load_missing(&mut self.playlist_player, wgpu);

		// If we're paused, don't update anything
		if self.paused {
			return;
		}

		let delta = TimeDelta::from_std(delta).expect("Duration since last update didn't fit into a time delta");
		self.step(wgpu, delta);
	}

	pub fn render(
		&mut self,
		shared: &Shared,
		wgpu: &Arc<Wgpu>,
		surface_geometry: Rect<i32, u32>,
		render_pass: &mut wgpu::RenderPass<'_>,
	) {
		let p = self.progress_norm();
		let f = self.fade_duration_norm();

		// Full duration an image is on screen (including the fades)
		let d = 1.0 + 2.0 * f;

		for geometry in &mut self.geometries {
			let image_uniforms = |image: Option<&Image>, image_slot| -> uniform::fade::Image {
				let Some(image) = image else {
					return uniform::fade::Image {
						image_ratio: uniform::Vec2([1.0, 1.0]),
						progress:    0.0,
						alpha:       0.0,
					};
				};

				let progress = match image_slot {
					ImageSlot::Prev => 1.0 - f32::max((f - p) / d, 0.0),
					ImageSlot::Cur => (p + f) / d,
					ImageSlot::Next => f32::max((p - 1.0 + f) / d, 0.0),
				};
				let progress = match image.swap_dir {
					true => 1.0 - progress,
					false => progress,
				};

				let p_stage = zsw_util::cmp_interval(p, f, 1.0 - f);
				let alpha = match p_stage {
					cmp::Ordering::Less => {
						let a = 0.5 + p / (2.0 * f);
						match image_slot {
							ImageSlot::Prev => 1.0 - a,
							ImageSlot::Cur => a,
							ImageSlot::Next => 0.0,
						}
					},
					cmp::Ordering::Equal => match image_slot {
						ImageSlot::Prev | ImageSlot::Next => 0.0,
						ImageSlot::Cur => 1.0,
					},
					cmp::Ordering::Greater => {
						let a = (p - (1.0 - f)) / (2.0 * f);
						match image_slot {
							ImageSlot::Prev => 0.0,
							ImageSlot::Cur => 1.0 - a,
							ImageSlot::Next => a,
						}
					},
				};

				// Calculate the position matrix for the panel
				let image_size = image.texture_view.texture().size();
				let image_size = Vector2D::new(image_size.width, image_size.height);
				let image_ratio = geometry::image_ratio(geometry.rect, image_size);

				uniform::fade::Image {
					image_ratio: uniform::Vec2(image_ratio.into()),
					progress,
					alpha,
				}
			};

			let images = uniform::fade::Images {
				prev: image_uniforms(self.images.prev.as_ref(), ImageSlot::Prev),
				cur:  image_uniforms(self.images.cur.as_ref(), ImageSlot::Cur),
				next: image_uniforms(self.images.next.as_ref(), ImageSlot::Next),
			};

			let geometry_uniforms = geometry.images.uniforms(wgpu, &shared.images);
			let pos_matrix = geometry::pos_matrix(geometry.rect, surface_geometry);
			let pos_matrix = uniform::Matrix4x4(pos_matrix.to_arrays());
			match self.kind {
				Kind::Basic => wgpu.write_buffer(&geometry_uniforms.buffer, &uniform::fade::Basic {
					pos_matrix,
					images,
					_unused: [0; _],
				}),
				Kind::Out { strength } => wgpu.write_buffer(&geometry_uniforms.buffer, &uniform::fade::Out {
					pos_matrix,
					images,
					strength,
					_unused: [0; _],
				}),
			}

			// Bind the geometry uniforms
			render_pass.set_bind_group(0, &geometry_uniforms.bind_group, &[]);

			// Bind the image uniforms
			let sampler = self.images.image_sampler(wgpu);
			render_pass.set_bind_group(1, self.images.bind_group(wgpu, sampler, &shared.images), &[]);

			render_pass.draw_indexed(0..6, 0, 0..1);
		}
	}

	#[expect(unused_results, reason = "egui")]
	pub fn draw_editor(&mut self, ui: &mut egui::Ui, wgpu: &Arc<Wgpu>, surface_geometry: Rect<i32, u32>) {
		{
			let mut is_paused = self.is_paused();
			ui.checkbox(&mut is_paused, "Paused");
			self.set_paused(is_paused);
		}

		ui.collapsing("Geometries", |ui| {
			for (geometry_idx, geometry) in self.geometries.iter().enumerate() {
				ui.horizontal(|ui| {
					let mut name = egui::WidgetText::from(format!("#{}: ", geometry_idx + 1));
					if !geometry.rect.intersects(surface_geometry) {
						name = name.weak();
					}

					ui.label(name);
					menu::draw_rect(ui, geometry.rect);
				});
			}
		});

		ui.horizontal(|ui| {
			ui.label("Cur progress");

			// Note: We only allow up until the duration - 1 so that you don't get stuck
			//       skipping images when you hold it at the max value
			// TODO: This max needs to be `duration - min_frame_duration` to not skip ahead.
			let max = self.duration.mul_f32(0.99);
			let mut progress = self.progress;
			menu::draw_duration(ui, &mut progress, Duration::ZERO..=max);
			self.set_progress(progress);
		});

		ui.horizontal(|ui| {
			ui.label("Fade Duration");
			let min = Duration::ZERO;
			let max = self.duration / 2;

			let mut fade_duration = self.fade_duration();
			menu::draw_duration(ui, &mut fade_duration, min..=max);
			self.set_fade_duration(fade_duration);
		});

		ui.horizontal(|ui| {
			ui.label("Duration");

			let mut duration = self.duration;
			menu::draw_duration(ui, &mut duration, Duration::ZERO..=Duration::from_secs_f32(180.0));
			self.set_duration(duration);
		});

		ui.horizontal(|ui| {
			ui.label("Skip");
			if ui.button("🔄").clicked() {
				self.skip(wgpu);
			}
		});

		ui.collapsing("Images", |ui| {
			self.draw_image_editor(ui, ImageSlot::Prev);
			self.draw_image_editor(ui, ImageSlot::Cur);
			self.draw_image_editor(ui, ImageSlot::Next);
		});
	}

	#[expect(unused_results, reason = "egui")]
	fn draw_image_editor(&mut self, ui: &mut egui::Ui, slot: ImageSlot) {
		ui.horizontal(|ui| {
			_ = match slot {
				ImageSlot::Prev => ui.weak("Previous"),
				ImageSlot::Cur => ui.weak("Current"),
				ImageSlot::Next => ui.weak("Next"),
			};

			let image = match slot {
				ImageSlot::Prev => &mut self.images.prev,
				ImageSlot::Cur => &mut self.images.cur,
				ImageSlot::Next => &mut self.images.next,
			};

			match image {
				Some(image) => image.draw_editor(ui),
				None => _ = ui.weak("[Unloaded]"),
			}
		});
	}
}

/// Geometry
#[derive(Debug)]
pub struct Geometry {
	rect:   Rect<i32, u32>,
	images: images::Geometry,
}

impl Geometry {
	pub fn new(rect: Rect<i32, u32>) -> Self {
		Self {
			rect,
			images: images::Geometry { uniforms: None },
		}
	}
}

/// Shared
#[derive(Debug)]
pub struct Shared {
	/// Images
	pub images: images::Shared,
}

impl Shared {
	/// Creates the shared
	pub fn new() -> Self {
		Self {
			images: images::Shared::new(),
		}
	}
}

/// Kind
#[derive(PartialEq, Clone, Copy, Debug)]
pub enum Kind {
	Basic,
	Out { strength: f32 },
}

impl Kind {
	/// Returns this kind's name
	pub fn name(self) -> &'static str {
		match self {
			Self::Basic => "Fade",
			Self::Out { .. } => "Fade out",
		}
	}

	/// Returns this kind's module as json
	pub fn module_json(self) -> &'static str {
		match self {
			Self::Basic => include_str!(concat!(env!("OUT_DIR"), "/shaders/panels/fade.json")),
			Self::Out { .. } => include_str!(concat!(env!("OUT_DIR"), "/shaders/panels/fade-out.json")),
		}
	}
}

/// Converts a chrono time delta into a duration, indicating whether it's positive or negative
fn time_delta_to_duration(delta: TimeDelta) -> (Duration, bool) {
	match delta.to_std() {
		Ok(delta) => (delta, true),
		Err(_) => ((-delta).to_std().expect("Duration should fit"), false),
	}
}
