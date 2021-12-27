//! Image handling


// Modules
mod loader;
mod processor;
mod request;
mod uvs;

// Exports
pub use loader::{ImageLoader, LoadedImageReceiver};
pub use processor::{ImageProcessor, ProcessedImageReceiver};
pub use request::ImageRequest;
pub use uvs::ImageUvs;

/// Loaded image
pub type LoadedImage = image::DynamicImage;

/// Processed image
pub type ProcessedImage = image::ImageBuffer<image::Rgba<u8>, Vec<u8>>;
