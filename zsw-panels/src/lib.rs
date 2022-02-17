//! Panel

// Features
#![feature(derive_default_enum)]
// Lints
#![warn(
	clippy::pedantic,
	clippy::nursery,
	missing_copy_implementations,
	missing_debug_implementations,
	noop_method_call,
	unused_results
)]
#![deny(
	// We want to annotate unsafe inside unsafe fns
	unsafe_op_in_unsafe_fn,
	// We muse use `expect` instead
	clippy::unwrap_used
)]
#![allow(
	// Style
	clippy::implicit_return,
	clippy::multiple_inherent_impl,
	clippy::pattern_type_mismatch,
	// `match` reads easier than `if / else`
	clippy::match_bool,
	clippy::single_match_else,
	//clippy::single_match,
	clippy::self_named_module_files,
	clippy::items_after_statements,
	clippy::module_name_repetitions,
	// Performance
	clippy::suboptimal_flops, // We prefer readability
	// Some functions might return an error in the future
	clippy::unnecessary_wraps,
	// Due to working with windows and rendering, which use `u32` / `f32` liberally
	// and interchangeably, we can't do much aside from casting and accepting possible
	// losses, although most will be lossless, since we deal with window sizes and the
	// such, which will fit within a `f32` losslessly.
	clippy::cast_precision_loss,
	clippy::cast_possible_truncation,
	// We use proper error types when it matters what errors can be returned, else,
	// such as when using `anyhow`, we just assume the caller won't check *what* error
	// happened and instead just bubbles it up
	clippy::missing_errors_doc,
	// Too many false positives and not too important
	clippy::missing_const_for_fn,
	// This is a binary crate, so we don't expose any API
	rustdoc::private_intra_doc_links,
)]

// Modules
mod image;
mod panel;
mod renderer;
mod state;

// Exports
pub use self::{
	image::PanelImage,
	panel::Panel,
	renderer::{PanelUniforms, PanelVertex, PanelsRenderer},
	state::{PanelState, PanelStateImage, PanelStateImages},
};

// Imports
use {
	anyhow::Context,
	cgmath::Point2,
	crossbeam::atomic::AtomicCell,
	futures::lock::{Mutex, MutexGuard},
	winit::dpi::{PhysicalPosition, PhysicalSize},
	zsw_img::ImageLoader,
	zsw_side_effect_macros::side_effect,
	zsw_util::{extse::AsyncLockMutexSe, MightBlock},
	zsw_wgpu::Wgpu,
};


/// Panels
#[derive(Debug)]
pub struct Panels {
	/// Panels renderer
	renderer: PanelsRenderer,

	/// All panels with their state
	panels: Mutex<Vec<PanelState>>,

	/// Lock source
	lock_source: LockSource,

	/// Current cursor position
	// TODO: Don't store this and instead request elsewhere
	cursor_pos: AtomicCell<Option<Point2<i32>>>,
}

impl Panels {
	/// Creates the panel
	pub fn new(device: &wgpu::Device, surface_texture_format: wgpu::TextureFormat) -> Result<Self, anyhow::Error> {
		// Create the renderer
		let renderer = PanelsRenderer::new(device, surface_texture_format).context("Unable to create renderer")?;

		Ok(Self {
			renderer,
			panels: Mutex::new(vec![]),
			lock_source: LockSource,
			cursor_pos: AtomicCell::new(None),
		})
	}

	/// Creates a panels lock
	///
	/// # Blocking
	/// Will block until any existing panels locks are dropped
	#[side_effect(MightBlock)]
	pub async fn lock_panels<'a>(&'a self) -> PanelsLock<'a> {
		// DEADLOCK: Caller is responsible to ensure we don't deadlock
		//           We don't lock it outside of this method
		let guard = self.panels.lock_se().await.allow::<MightBlock>();
		PanelsLock::new(guard, &self.lock_source)
	}

	/// Sets the cursor position
	pub fn set_cursor_pos(&self, cursor_pos: PhysicalPosition<f64>) {
		// Convert to `i32`
		// TODO: Check if doing this is fine
		let cursor_pos = Point2::new(cursor_pos.x as i32, cursor_pos.y as i32);

		// Then set it
		self.cursor_pos.store(Some(cursor_pos));
	}

	/// Adds a new panel
	pub fn add_panel(&self, panels_lock: &mut PanelsLock, panel: Panel) {
		panels_lock.get_mut(&self.lock_source).push(PanelState::new(panel));
	}

	/// Returns all panels
	pub fn panels<'a>(&self, panels_lock: &'a PanelsLock) -> &'a [PanelState] {
		panels_lock.get(&self.lock_source)
	}

	/// Returns all panels, mutably
	pub fn panels_mut<'a>(&self, panels_lock: &'a mut PanelsLock) -> &'a mut [PanelState] {
		panels_lock.get_mut(&self.lock_source)
	}

	/// Replaces all panels
	pub fn replace_panels(&self, panels_lock: &mut PanelsLock, panels: impl IntoIterator<Item = Panel>) {
		*panels_lock.get_mut(&self.lock_source) = panels.into_iter().map(PanelState::new).collect();
	}

	/// Updates all panels
	pub fn update_all(
		&self,
		panels_lock: &mut PanelsLock,
		wgpu: &Wgpu,
		image_loader: &ImageLoader,
	) -> Result<(), anyhow::Error> {
		let panels = panels_lock.get_mut(&self.lock_source);

		for panel in &mut *panels {
			panel
				.update(&self.renderer, wgpu, image_loader)
				.context("Unable to update panel")?;
		}

		Ok(())
	}

	/// Renders all panels
	pub fn render(
		&self,
		panels_lock: &PanelsLock,
		queue: &wgpu::Queue,
		encoder: &mut wgpu::CommandEncoder,
		surface_view: &wgpu::TextureView,
		surface_size: PhysicalSize<u32>,
	) -> Result<(), anyhow::Error> {
		let panels = panels_lock.get(&self.lock_source);

		// Then render
		self.renderer.render(
			panels,
			self.cursor_pos.load().unwrap_or(Point2::new(0, 0)),
			queue,
			encoder,
			surface_view,
			surface_size,
		)
	}
}

/// Source for all locks
// Note: This is to ensure user can't create the locks themselves
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct LockSource;

/// Panels lock
pub type PanelsLock<'a> = zsw_util::Lock<'a, MutexGuard<'a, Vec<PanelState>>, LockSource>;
