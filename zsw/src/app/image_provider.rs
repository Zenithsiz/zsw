//! Image provider for `ImageLoader`

// Imports
use {
	anyhow::Context,
	std::{fs, io, path::Path, sync::Arc},
	zsw_playlist::{PlaylistImage, PlaylistReceiver},
};

/// Image provider based on `PlaylistReceiver` for `ImageLoader`
#[derive(Clone, Debug)]
pub struct ImageProvider {
	/// Playlist receiver
	playlist_receiver: PlaylistReceiver,
}

impl ImageProvider {
	/// Creates a new image provider
	pub fn new(playlist_receiver: PlaylistReceiver) -> Self {
		Self { playlist_receiver }
	}
}

impl zsw_img::service::RawImageProvider for ImageProvider {
	type RawImage = RawImage;

	fn next_image(&self) -> Option<Self::RawImage> {
		// Keep requesting until we manage to load it (or we're out of them)
		let image = loop {
			let playlist_image = self.playlist_receiver.next()?;

			match self::open_image(&playlist_image) {
				Ok(reader) =>
					break RawImage {
						reader,
						name: self::image_name(&playlist_image),
						playlist_image,
					},
				Err(err) => {
					tracing::warn!("Unable to load image: {err:?}");
					self.playlist_receiver.remove_image(playlist_image);
					continue;
				},
			}
		};

		Some(image)
	}

	fn remove_image(&self, raw_image: &Self::RawImage) {
		self.playlist_receiver
			.remove_image(Arc::clone(&raw_image.playlist_image));
	}
}

/// Raw image for `RawImageProvider::RawImage`
pub struct RawImage {
	/// Reader
	reader: io::BufReader<fs::File>,

	/// Name
	name: String,

	/// Playlist image
	playlist_image: Arc<PlaylistImage>,
}

impl zsw_img::service::RawImage for RawImage {
	type Reader<'a> = &'a mut io::BufReader<fs::File>
	where
		Self: 'a,;

	fn reader(&mut self) -> Self::Reader<'_> {
		&mut self.reader
	}

	fn name(&self) -> &str {
		&self.name
	}
}

/// Returns the name of an image
fn image_name(playlist_image: &PlaylistImage) -> String {
	match playlist_image {
		PlaylistImage::File(path) => format!("file://{}", path.display()),
	}
}

/// Tries to open an image
fn open_image(playlist_image: &PlaylistImage) -> Result<io::BufReader<fs::File>, anyhow::Error> {
	match playlist_image {
		PlaylistImage::File(path) =>
			self::open_fs_image(path).with_context(|| format!("Unable to load image {path:?}")),
	}
}

/// Tries to open a filesystem-image
fn open_fs_image(path: &Path) -> Result<io::BufReader<fs::File>, anyhow::Error> {
	let path = path.canonicalize().context("Unable to canonicalize file")?;
	let file = fs::File::open(path).context("Unable to open file")?;
	Ok(io::BufReader::new(file))
}
