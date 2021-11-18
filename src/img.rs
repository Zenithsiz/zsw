//! Image handling


// Modules
mod loader;
mod processor;
mod request;
mod uvs;

// Exports
pub use loader::{ImageLoader, RawImageReceiver};
pub use processor::{ImageProcessor, ProcessedImageReceiver};
pub use request::ImageRequest;
pub use uvs::ImageUvs;

/// Raw image
pub type RawImage = image::DynamicImage;

/// Processed image
pub type ProcessedImage = image::ImageBuffer<image::Rgba<u8>, Vec<u8>>;
