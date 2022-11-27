//! Profile applier

// Imports
use {
	super::Services,
	std::path::PathBuf,
	zsw_panels::{Panel, PanelsShader},
	zsw_profiles::{profile, Profile},
};

/// Profile applier
#[derive(Clone, Debug)]
pub struct ProfileApplier {}

impl ProfileApplier {
	/// Creates a new profile applier
	pub fn new() -> Self {
		Self {}
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
		}
	}

	/// Converts a `PanelsShader` to `profile::PanelsShader`
	fn dump_shader(panels_shader: PanelsShader) -> profile::PanelsShader {
		match panels_shader {
			PanelsShader::Fade => profile::PanelsShader::Fade,
			PanelsShader::FadeWhite { strength } => profile::PanelsShader::FadeWhite { strength },
		}
	}
}

impl zsw_settings_window::ProfileApplier<Services> for ProfileApplier {
	fn apply(
		&mut self,
		profile: &zsw_profiles::Profile,
		services: &Services,
		panels_resource: &mut zsw_panels::PanelsResource,
	) {
		// TODO: Also flush out all image buffers
		tracing::debug!(?profile, "Applying profile");
		services.playlist_manager.set_root_path(profile.root_path.clone());
		services
			.panels_editor
			.replace_panels(panels_resource, profile.panels.iter().map(Self::create_panel));
		services
			.panels_editor
			.set_max_image_size(panels_resource, profile.max_image_size);
		services
			.panels_editor
			.set_shader(panels_resource, Self::create_shader(profile.panels_shader));
	}

	fn current(
		&mut self,
		services: &Services,
		panels_resource: &mut zsw_panels::PanelsResource,
	) -> zsw_profiles::Profile {
		Profile {
			root_path:      match services.playlist_manager.root_path() {
				Some(path) => path,
				// TODO: What to do here?
				None => {
					tracing::warn!("No root path was set");
					PathBuf::from("<not set>")
				},
			},
			panels:         services
				.panels_editor
				.panels(panels_resource)
				.iter()
				.map(|panel| Self::dump_panel(&panel.panel))
				.collect(),
			max_image_size: services.panels_editor.max_image_size(panels_resource),
			panels_shader:  Self::dump_shader(services.panels_editor.shader(panels_resource)),
		}
	}
}
