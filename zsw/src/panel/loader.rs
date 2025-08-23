//! Panels loader

// Imports
use {
	super::{Panel, PanelName, PanelShader, PanelState, ser},
	crate::AppError,
	std::path::PathBuf,
	zsw_util::PathAppendExt,
	zutil_app_error::Context,
};

/// Panels loader
#[derive(Debug)]
pub struct PanelsLoader {
	/// Panels directory
	root: PathBuf,
}

impl PanelsLoader {
	/// Creates a new panels loader
	pub fn new(root: PathBuf) -> Self {
		Self { root }
	}

	/// Loads a panel from a name.
	///
	/// If the panel isn't for this window, returns `Ok(None)`
	pub async fn load(&self, panel_name: PanelName, shader: PanelShader) -> Result<Panel, AppError> {
		// Try to read the file
		let panel_path = self.panel_path(&panel_name);
		tracing::debug!(%panel_name, ?panel_path, "Loading panel");
		let panel_toml = tokio::fs::read_to_string(panel_path)
			.await
			.context("Unable to open file")?;

		// Then parse it
		let panel = toml::from_str::<ser::Panel>(&panel_toml).context("Unable to parse panel")?;

		// Finally convert it
		let geometries = panel.geometries.into_iter().map(|geometry| geometry.geometry).collect();
		let state = PanelState {
			paused:     false,
			progress:   0,
			duration:   panel.state.duration,
			fade_point: panel.state.fade_point,
		};

		let panel = Panel::new(panel_name.clone(), geometries, state, shader);

		Ok(panel)
	}

	/// Returns a panel's path
	pub fn panel_path(&self, name: &PanelName) -> PathBuf {
		self.root.join(&*name.0).with_appended(".toml")
	}
}
