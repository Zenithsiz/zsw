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

type DisplayStorage = Arc<OnceCell<Arc<Mutex<Display>>>>;

/// Displays
#[derive(Debug)]
pub struct Displays {
	/// Displays directory
	root: PathBuf,

	/// Loaded displays
	// TODO: Limit the size of this?
	displays: Mutex<HashMap<DisplayName, DisplayStorage>>,
}

impl Displays {
	/// Creates a new displays container
	pub fn new(root: PathBuf) -> Self {
		Self {
			root,
			displays: Mutex::new(HashMap::new()),
		}
	}

	/// Adds a new display
	pub async fn add(&self, display_name: DisplayName, display: Display) -> Arc<Mutex<Display>> {
		let display = Arc::new(Mutex::new(display));
		_ = self
			.displays
			.lock()
			.await
			.insert(display_name, Arc::new(OnceCell::new_with(Some(Arc::clone(&display)))));

		display
	}

	/// Loads a display by name
	pub async fn load(&self, display_name: DisplayName) -> Result<Arc<Mutex<Display>>, AppError> {
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

				Ok(Arc::new(Mutex::new(display)))
			})
			.await
			.map(Arc::clone)
	}

	/// Saves a display by name
	pub async fn save(&self, display_name: &DisplayName) -> Result<(), AppError> {
		let display_path = self.path_of(display_name);
		tracing::debug!("Saving display {display_name:?} to {display_path:?}");

		let display = {
			let displays = self.displays.lock().await;

			let display = displays
				.get(display_name)
				.context("Unknown display name")?
				.get()
				.context("Display is still initializing")?;

			Arc::clone(display)
		};
		let display = display.lock().await;

		let display = ser::Display {
			geometries: display
				.geometries
				.iter()
				.map(|&geometry| ser::DisplayGeometry { geometry })
				.collect(),
		};

		let display = toml::to_string_pretty(&display).context("Unable to serialize display")?;
		tokio::fs::write(&display_path, &display)
			.await
			.context("Unable to write display")?;

		Ok(())
	}

	/// Returns all displays
	pub async fn get_all(&self) -> Vec<Arc<Mutex<Display>>> {
		self.displays
			.lock()
			.await
			.values()
			.filter_map(|display| display.get().map(Arc::clone))
			.collect()
	}

	/// Returns a display's path
	pub fn path_of(&self, name: &DisplayName) -> PathBuf {
		self.root.join(&*name.0).with_added_extension("toml")
	}
}
