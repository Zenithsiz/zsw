//! Panels

use {
	super::{Panel, shader},
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
					Panel::None(panel::shader::none::Shader::new(geometries, shader.background_color)),
				ProfilePanelShader::Fade(shader) => {
					let playlist_player = PlaylistPlayer::new(&playlists[&shader.playlist])
						.with_context(|| format!("Unable to load playlist {:?}", shader.playlist))?;

					let state = panel::shader::fade::Shader::new(
						geometries,
						shader.duration,
						shader.fade_duration,
						playlist_player,
						match shader.kind {
							ProfilePanelFadeShaderKind::Basic => panel::shader::fade::Kind::Basic,
							ProfilePanelFadeShaderKind::Out { strength } => panel::shader::fade::Kind::Out { strength },
						},
					);

					Panel::Fade(state)
				},
				ProfilePanelShader::Slide(shader) => {
					let playlist_player = PlaylistPlayer::new(&playlists[&shader.playlist])
						.with_context(|| format!("Unable to load playlist {:?}", shader.playlist))?;

					let dir = match shader.dir {
						ProfilePanelSlideDir::LeftRight => shader::slide::Dir::LeftRight,
						ProfilePanelSlideDir::RightLeft => shader::slide::Dir::RightLeft,
						ProfilePanelSlideDir::UpDown => shader::slide::Dir::UpDown,
						ProfilePanelSlideDir::DownUp => shader::slide::Dir::DownUp,
					};

					let state = panel::shader::slide::Shader::new(
						geometries,
						shader.duration,
						playlist_player,
						dir,
						match shader.kind {
							ProfilePanelSlideShaderKind::Basic => panel::shader::slide::Kind::Basic,
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
