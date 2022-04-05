//! Image handling

// Features
#![feature(never_type)]
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
pub mod loader;

// Exports
pub use loader::ImageLoader;

// Imports
use {
	cgmath::Vector2,
	image::{DynamicImage, GenericImageView},
	std::path::PathBuf,
};

/// Loaded image
#[derive(Debug)]
pub struct Image {
	/// Path of the image
	pub path: PathBuf,

	/// Image
	pub image: DynamicImage,
}

impl Image {
	/// Returns the image's size
	#[must_use]
	pub fn size(&self) -> Vector2<u32> {
		Vector2::new(self.image.width(), self.image.height())
	}
}
