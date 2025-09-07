//! Panels

// Imports
use {
	super::{Panel, PanelName, PanelState, ser},
	crate::{
		AppError,
		panel::{PanelNoneState, PanelShaderFade, state::PanelFadeState},
	},
	app_error::Context,
	futures::lock::Mutex,
	std::{collections::HashMap, path::PathBuf, sync::Arc},
	tokio::sync::OnceCell,
};

/// Panel storage
type PanelStorage = Arc<Mutex<Panel>>;

/// Panels
#[derive(Debug)]
pub struct Panels {
	/// Panels directory
	root: PathBuf,

	/// Loaded panels
	// TODO: Limit the size of this?
	panels: Mutex<HashMap<PanelName, Arc<OnceCell<PanelStorage>>>>,
}

impl Panels {
	/// Creates a new panels container
	pub fn new(root: PathBuf) -> Self {
		Self {
			root,
			panels: Mutex::new(HashMap::new()),
		}
	}

	/// Loads a panel from a name.
	///
	/// If the panel isn't for this window, returns `Ok(None)`
	pub async fn load(&self, panel_name: PanelName) -> Result<Arc<Mutex<Panel>>, AppError> {
		let panel_entry = Arc::clone(
			self.panels
				.lock()
				.await
				.entry(panel_name.clone())
				.or_insert_with(|| Arc::new(OnceCell::new())),
		);

		panel_entry
			.get_or_try_init(async move || {
				// Try to read the file
				let panel_path = self.path_of(&panel_name);
				tracing::debug!("Loading panel {panel_name:?} from {panel_path:?}");
				let panel_toml = tokio::fs::read_to_string(panel_path)
					.await
					.context("Unable to open file")?;

				// Then parse it
				let panel = toml::from_str::<ser::Panel>(&panel_toml).context("Unable to parse panel")?;

				// Finally convert it
				let geometries = panel.geometries.into_iter().map(|geometry| geometry.geometry).collect();
				let state = match panel.shader {
					ser::PanelShader::None { background_color } =>
						PanelState::None(PanelNoneState::new(background_color)),
					ser::PanelShader::Fade(fade) => PanelState::Fade(PanelFadeState::new(
						panel.state.duration,
						panel.state.fade_duration,
						match fade {
							ser::PanelShaderFade::Basic => PanelShaderFade::Basic,
							ser::PanelShaderFade::White { strength } => PanelShaderFade::White { strength },
							ser::PanelShaderFade::Out { strength } => PanelShaderFade::Out { strength },
							ser::PanelShaderFade::In { strength } => PanelShaderFade::In { strength },
						},
					)),
				};
				let panel = Panel::new(panel_name.clone(), geometries, state);

				Ok(Arc::new(Mutex::new(panel)))
			})
			.await
			.map(Arc::clone)
	}

	/// Returns all panels
	pub async fn get_all(&self) -> Vec<PanelStorage> {
		self.panels
			.lock()
			.await
			.values()
			.filter_map(|panel| panel.get())
			.map(Arc::clone)
			.collect()
	}

	/// Returns a panel's path
	pub fn path_of(&self, name: &PanelName) -> PathBuf {
		self.root.join(&*name.0).with_added_extension("toml")
	}
}
