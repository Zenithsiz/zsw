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
	std::sync::{
		Arc,
		nonpoison::{MappedMutexGuard, Mutex, MutexGuard},
	},
	zsw_util::AppError,
};

/// Inner
#[derive(Debug)]
struct Inner {
	/// Profile name
	profile_name: Option<ProfileName>,

	/// Panels
	panels: Vec<Panel>,
}

/// Panels
#[derive(Debug)]
pub struct Panels {
	/// Inner
	inner: Mutex<Inner>,
}

impl Panels {
	/// Creates the panels with no current profile
	pub fn new() -> Self {
		Self {
			inner: Mutex::new(Inner {
				profile_name: None,
				panels:       vec![],
			}),
		}
	}

	/// Gets the panels
	pub fn get_all(&self) -> MappedMutexGuard<'_, [Panel]> {
		MutexGuard::map(self.inner.lock(), |inner| inner.panels.as_mut_slice())
	}

	/// Sets the current profile.
	///
	/// If a profile already exists, unloads it's panels first
	pub fn set_profile(
		&self,
		profile_name: ProfileName,
		profile: &Profile,
		playlists: &Arc<Playlists>,
	) -> Result<(), AppError> {
		let mut inner = self.inner.lock();
		inner.profile_name = Some(profile_name);
		inner.panels.clear();
		for profile_panel in &profile.panels {
			let state = match &profile_panel.shader {
				ProfilePanelShader::None(shader) => PanelState::None(PanelNoneState::new(shader.background_color)),
				ProfilePanelShader::Fade(shader) => {
					let mut playlist_player = PlaylistPlayer::new();

					// TODO: This is called for each panel, which might load the same playlist
					//       multiple times, we should instead load and cache the playlist itself
					let playlist = &playlists[&shader.playlist];
					playlist_player
						.load(playlist)
						.with_context(|| format!("Unable to load playlist {:?}", shader.playlist))?;

					let state = PanelFadeState::new(
						shader.duration,
						shader.fade_duration,
						playlist_player,
						match shader.inner {
							ProfilePanelFadeShaderInner::Basic => PanelFadeShader::Basic,
							ProfilePanelFadeShaderInner::White { strength } => PanelFadeShader::White { strength },
							ProfilePanelFadeShaderInner::Out { strength } => PanelFadeShader::Out { strength },
							ProfilePanelFadeShaderInner::In { strength } => PanelFadeShader::In { strength },
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

			let panel = Panel::new(profile_panel.display_name.clone(), state);
			inner.panels.push(panel);
		}

		Ok(())
	}
}
