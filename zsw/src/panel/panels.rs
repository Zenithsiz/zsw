//! Panels

use {
	super::{Panel, shader},
	crate::{
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
			let panel =
				match &profile_panel.shader {
					ProfilePanelShader::None(shader) => {
						let geometries = profile_panel
							.geometries
							.iter()
							.map(|geometry| shader::none::Geometry::new(geometry.geometry))
							.collect();
						Panel::None(shader::none::Shader::new(geometries, shader.background_color))
					},
					ProfilePanelShader::Fade(shader) => {
						let playlist_player = PlaylistPlayer::new(&playlists[&shader.playlist])
							.with_context(|| format!("Unable to load playlist {:?}", shader.playlist))?;

						let geometries = profile_panel
							.geometries
							.iter()
							.map(|geometry| shader::fade::Geometry::new(geometry.geometry))
							.collect();

						let shader = shader::fade::Shader::new(
							geometries,
							shader.duration,
							shader.fade_duration,
							playlist_player,
							match shader.kind {
								ProfilePanelFadeShaderKind::Basic => shader::fade::Kind::Basic,
								ProfilePanelFadeShaderKind::Out { strength } => shader::fade::Kind::Out { strength },
							},
						);

						Panel::Fade(shader)
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

						let geometries = profile_panel
							.geometries
							.iter()
							.map(|geometry| shader::slide::Geometry::new(geometry.geometry))
							.collect();

						let shader =
							shader::slide::Shader::new(geometries, shader.duration, playlist_player, dir, match shader
								.kind
							{
								ProfilePanelSlideShaderKind::Basic => shader::slide::Kind::Basic,
							});

						Panel::Slide(shader)
					},
				};

			self.panels.push(panel);
		}

		Ok(())
	}
}
