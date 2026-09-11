//! Panels

use {
	super::{Panel, state},
	crate::{
		panel,
		playlist::{PlaylistPlayer, Playlists},
		profile::{
			Profile,
			ProfileName,
			ProfilePanelFadeShaderKind,
			ProfilePanelShader,
			ProfilePanelSlideDir,
			ProfilePanelSlideShaderKind,
		},
	},
	app_error::Context,
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
		playlists: &Playlists,
	) -> Result<(), AppError> {
		self.profile_name = Some(profile_name);
		self.panels.clear();
		for profile_panel in &profile.panels {
			let geometries = profile_panel
				.geometries
				.iter()
				.map(|geometry| panel::Geometry::new(geometry.geometry))
				.collect();

			let panel = match &profile_panel.shader {
				ProfilePanelShader::None(shader) =>
					Panel::None(panel::state::none::State::new(geometries, shader.background_color)),
				ProfilePanelShader::Fade(shader) => {
					let playlist_player = PlaylistPlayer::new(&playlists[&shader.playlist])
						.with_context(|| format!("Unable to load playlist {:?}", shader.playlist))?;

					let state = panel::state::fade::State::new(
						geometries,
						shader.duration,
						shader.fade_duration,
						playlist_player,
						match shader.kind {
							ProfilePanelFadeShaderKind::Basic => panel::state::fade::Kind::Basic,
							ProfilePanelFadeShaderKind::Out { strength } => panel::state::fade::Kind::Out { strength },
						},
					);

					Panel::Fade(state)
				},
				ProfilePanelShader::Slide(shader) => {
					let playlist_player = PlaylistPlayer::new(&playlists[&shader.playlist])
						.with_context(|| format!("Unable to load playlist {:?}", shader.playlist))?;

					let dir = match shader.dir {
						ProfilePanelSlideDir::LeftRight => state::slide::Dir::LeftRight,
						ProfilePanelSlideDir::RightLeft => state::slide::Dir::RightLeft,
						ProfilePanelSlideDir::UpDown => state::slide::Dir::UpDown,
						ProfilePanelSlideDir::DownUp => state::slide::Dir::DownUp,
					};

					let state = panel::state::slide::State::new(
						geometries,
						shader.duration,
						playlist_player,
						dir,
						match shader.kind {
							ProfilePanelSlideShaderKind::Basic => panel::state::slide::Kind::Basic,
						},
					);

					Panel::Slide(state)
				},
			};

			self.panels.push(panel);
		}

		Ok(())
	}
}
