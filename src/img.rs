//! Image handling


// Modules
mod loader;
mod uvs;

// Exports
pub use loader::ImageLoader;
pub use uvs::ImageUvs;

/// Image
pub type Image = image::ImageBuffer<image::Rgba<u8>, Vec<u8>>;
