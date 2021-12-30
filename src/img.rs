//! Image handling


// Modules
mod loader;
mod request;
mod uvs;

// Exports
pub use loader::{ImageLoader, ImageLoaderArgs, ImageReceiver};
pub use request::ImageRequest;
pub use uvs::ImageUvs;

/// Image
pub type Image = image::ImageBuffer<image::Rgba<u8>, Vec<u8>>;
