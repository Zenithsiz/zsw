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
	// This is too prevalent on generic functions, which we don't want to ALWAYS be `Send`
	clippy::future_not_send,
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
	winit::dpi::PhysicalSize,
	zsw_img::ImageLoader,
	zsw_input::Input,
	zsw_wgpu::Wgpu,
};


/// Panels service
// TODO: Rename to `PanelsService`
#[derive(Debug)]
pub struct Panels {
	/// Panels renderer
	renderer: PanelsRenderer,
}

#[allow(clippy::unused_self)] // For accessing resources, we should require the service
impl Panels {
	/// Creates the panel, alongside it's resources
	pub fn new(
		device: &wgpu::Device,
		surface_texture_format: wgpu::TextureFormat,
	) -> Result<(Self, PanelsResource), anyhow::Error> {
		// Create the renderer
		let renderer = PanelsRenderer::new(device, surface_texture_format).context("Unable to create renderer")?;

		// Create the service
		let service = Self { renderer };

		// Create our resource
		let resource = PanelsResource { panels: vec![] };

		Ok((service, resource))
	}

	/// Adds a new panel
	pub fn add_panel(&self, resource: &mut PanelsResource, panel: Panel) {
		resource.panels.push(PanelState::new(panel));
	}

	/// Returns all panels
	pub fn panels<'a>(&self, resource: &'a PanelsResource) -> &'a [PanelState] {
		&resource.panels
	}

	/// Returns all panels, mutably
	pub fn panels_mut<'a>(&self, resource: &'a mut PanelsResource) -> &'a mut [PanelState] {
		&mut resource.panels
	}

	/// Replaces all panels
	pub fn replace_panels(&self, resource: &mut PanelsResource, panels: impl IntoIterator<Item = Panel>) {
		resource.panels = panels.into_iter().map(PanelState::new).collect();
	}

	/// Updates all panels
	pub fn update_all(
		&self,
		resource: &mut PanelsResource,
		wgpu: &Wgpu,
		image_loader: &ImageLoader,
	) -> Result<(), anyhow::Error> {
		for panel in &mut resource.panels {
			panel
				.update(&self.renderer, wgpu, image_loader)
				.context("Unable to update panel")?;
		}

		Ok(())
	}

	/// Renders all panels
	pub fn render(
		&self,
		input: &Input,
		resource: &PanelsResource,
		queue: &wgpu::Queue,
		encoder: &mut wgpu::CommandEncoder,
		surface_view: &wgpu::TextureView,
		surface_size: PhysicalSize<u32>,
	) -> Result<(), anyhow::Error> {
		let cursor_pos = input
			.cursor_pos()
			.map_or(Point2::new(0, 0), |pos| Point2::new(pos.x as i32, pos.y as i32));

		// Then render
		self.renderer
			.render(&resource.panels, cursor_pos, queue, encoder, surface_view, surface_size)
	}
}

/// Panels resource
#[derive(Debug)]
pub struct PanelsResource {
	/// All panels with their state
	panels: Vec<PanelState>,
}
