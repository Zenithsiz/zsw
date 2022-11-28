//! Profile applier

// Imports
use {
	std::path::PathBuf,
	zsw_panels::{Panel, PanelsEditor, PanelsShader},
	zsw_playlist::PlaylistManager,
	zsw_profiles::{profile, Profile},
};

/// Profile applier
#[derive(Clone, Debug)]
pub struct ProfileApplier {
	/// Playlist manager
	playlist_manager: PlaylistManager,

	/// Panels editor
	panels_editor: PanelsEditor,
}

impl ProfileApplier {
	/// Creates a new profile applier
	pub fn new(playlist_manager: PlaylistManager, panels_editor: PanelsEditor) -> Self {
		Self {
			playlist_manager,
			panels_editor,
		}
	}

	/// Converts a `profile::Panel` to a `Panel`
	fn create_panel(panel: &profile::Panel) -> Panel {
		Panel {
			geometry:         panel.geometry,
			duration:         panel.duration,
			fade_point:       panel.fade_point,
			parallax_ratio:   panel.parallax.ratio,
			parallax_exp:     panel.parallax.exp,
			reverse_parallax: panel.parallax.reverse,
		}
	}

	/// Converts a `Panel` to `profile::Panel`
	fn dump_panel(panel: &Panel) -> profile::Panel {
		profile::Panel {
			geometry:   panel.geometry,
			duration:   panel.duration,
			fade_point: panel.fade_point,
			parallax:   profile::PanelParallax {
				ratio:   panel.parallax_ratio,
				exp:     panel.parallax_exp,
				reverse: panel.reverse_parallax,
			},
		}
	}

	/// Converts a `profile::PanelsShader` to a `PanelsShader`
	fn create_shader(panels_shader: profile::PanelsShader) -> PanelsShader {
		match panels_shader {
			profile::PanelsShader::Fade => PanelsShader::Fade,
			profile::PanelsShader::FadeWhite { strength } => PanelsShader::FadeWhite { strength },
			profile::PanelsShader::FadeOut { strength } => PanelsShader::FadeOut { strength },
			profile::PanelsShader::FadeIn { strength } => PanelsShader::FadeIn { strength },
		}
	}

	/// Converts a `PanelsShader` to `profile::PanelsShader`
	fn dump_shader(panels_shader: PanelsShader) -> profile::PanelsShader {
		match panels_shader {
			PanelsShader::Fade => profile::PanelsShader::Fade,
			PanelsShader::FadeWhite { strength } => profile::PanelsShader::FadeWhite { strength },
			PanelsShader::FadeOut { strength } => profile::PanelsShader::FadeOut { strength },
			PanelsShader::FadeIn { strength } => profile::PanelsShader::FadeIn { strength },
		}
	}
}

impl zsw_settings_window::ProfileApplier for ProfileApplier {
	fn apply(&mut self, profile: &zsw_profiles::Profile, panels_resource: &mut zsw_panels::PanelsResource) {
		// TODO: Also flush out all image buffers
		tracing::debug!(?profile, "Applying profile");
		self.playlist_manager.set_root_path(profile.root_path.clone());
		self.panels_editor
			.replace_panels(panels_resource, profile.panels.iter().map(Self::create_panel));
		self.panels_editor
			.set_max_image_size(panels_resource, profile.max_image_size);
		self.panels_editor
			.set_shader(panels_resource, Self::create_shader(profile.panels_shader));
	}

	fn current(&mut self, panels_resource: &mut zsw_panels::PanelsResource) -> zsw_profiles::Profile {
		Profile {
			root_path:      match self.playlist_manager.root_path() {
				Some(path) => path,
				// TODO: What to do here?
				None => {
					tracing::warn!("No root path was set");
					PathBuf::from("<not set>")
				},
			},
			panels:         self
				.panels_editor
				.panels(panels_resource)
				.iter()
				.map(|panel| Self::dump_panel(&panel.panel))
				.collect(),
			max_image_size: self.panels_editor.max_image_size(panels_resource),
			panels_shader:  Self::dump_shader(self.panels_editor.shader(panels_resource)),
		}
	}
}
