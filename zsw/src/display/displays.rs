//! Displays

// Imports
use {
	super::{Display, DisplayName, ser},
	app_error::Context,
	futures::lock::Mutex,
	std::{collections::HashMap, path::PathBuf, sync::Arc},
	tokio::sync::OnceCell,
	zsw_util::AppError,
};

/// Displays
#[derive(Debug)]
pub struct Displays {
	/// Displays directory
	root: PathBuf,

	/// Loaded displays
	// TODO: Limit the size of this?
	displays: Mutex<HashMap<DisplayName, Arc<OnceCell<Arc<Display>>>>>,
}

impl Displays {
	/// Creates a new displays container
	pub fn new(root: PathBuf) -> Self {
		Self {
			root,
			displays: Mutex::new(HashMap::new()),
		}
	}

	/// Loads a display by name
	pub async fn load(&self, display_name: DisplayName) -> Result<Arc<Display>, AppError> {
		let display_entry = Arc::clone(
			self.displays
				.lock()
				.await
				.entry(display_name.clone())
				.or_insert_with(|| Arc::new(OnceCell::new())),
		);

		display_entry
			.get_or_try_init(async move || {
				// Try to read the file
				let display_path = self.path_of(&display_name);
				tracing::debug!("Loading display {display_name:?} from {display_path:?}");
				let display_toml = tokio::fs::read_to_string(display_path)
					.await
					.context("Unable to open file")?;

				// And parse it
				let display = toml::from_str::<ser::Display>(&display_toml).context("Unable to parse display")?;
				let display = Display {
					name:       display_name.clone(),
					geometries: display
						.geometries
						.into_iter()
						.map(|geometry| geometry.geometry)
						.collect(),
				};
				tracing::info!("Loaded display {display_name:?}");

				Ok(Arc::new(display))
			})
			.await
			.map(Arc::clone)
	}

	/// Returns a display's path
	pub fn path_of(&self, name: &DisplayName) -> PathBuf {
		self.root.join(&*name.0).with_added_extension("toml")
	}
}
