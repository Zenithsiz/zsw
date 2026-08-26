//! Panels

// Imports
use {
	super::Panel,
	crate::{
		panel::{
			PanelFadeShader,
			PanelSlideShader,
			PanelState,
			state::{PanelFadeState, PanelNoneState, PanelSlideState},
		},
		playlist::{PlaylistPlayer, Playlists},
		profile::{
			Profile,
			ProfileName,
			ProfilePanelFadeShaderInner,
			ProfilePanelShader,
			ProfilePanelSlideShaderInner,
		},
	},
	app_error::Context,
	std::sync::Arc,
	zsw_util::AppError,
};

/// Panels
#[derive(Debug)]
pub struct Panels {
	/// Profile name
	profile_name: Option<ProfileName>,

	/// Panels
	panels: Vec<Panel>,
}

impl Panels {
	/// Creates the panels with no current profile
	pub fn new() -> Self {
		Self {
			profile_name: None,
			panels:       vec![],
		}
	}

	/// Gets the panels
	pub fn get_all(&mut self) -> &mut [Panel] {
		&mut self.panels
	}

	/// Sets the current profile.
	///
	/// If a profile already exists, unloads it's panels first
	pub fn set_profile(
		&mut self,
		profile_name: ProfileName,
		profile: &Profile,
		playlists: &Arc<Playlists>,
	) -> Result<(), AppError> {
		self.profile_name = Some(profile_name);
		self.panels.clear();
		for profile_panel in &profile.panels {
			let state = match &profile_panel.shader {
				ProfilePanelShader::None(shader) => PanelState::None(PanelNoneState::new(shader.background_color)),
				ProfilePanelShader::Fade(shader) => {
					let playlist_player = PlaylistPlayer::new(&playlists[&shader.playlist])
						.with_context(|| format!("Unable to load playlist {:?}", shader.playlist))?;

					let state = PanelFadeState::new(
						shader.duration,
						shader.fade_duration,
						playlist_player,
						match shader.inner {
							ProfilePanelFadeShaderInner::Basic => PanelFadeShader::Basic,
							ProfilePanelFadeShaderInner::Out { strength } => PanelFadeShader::Out { strength },
						},
					);

					PanelState::Fade(state)
				},
				ProfilePanelShader::Slide(shader) => {
					let state = PanelSlideState::new(match shader.inner {
						ProfilePanelSlideShaderInner::Basic => PanelSlideShader::Basic,
					});

					PanelState::Slide(state)
				},
			};

			self.panels.push(Panel {
				geometries: profile_panel.geometries.clone(),
				state,
			});
		}

		Ok(())
	}
}
