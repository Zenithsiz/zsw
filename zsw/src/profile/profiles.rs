//! Profiles

// Imports
use {
	super::{Profile, ProfileName, ser},
	crate::{
		panel::PanelName,
		playlist::PlaylistName,
		profile::{
			ProfilePanel,
			ProfilePanelFadeShader,
			ProfilePanelFadeShaderInner,
			ProfilePanelNoneShader,
			ProfilePanelShader,
		},
	},
	app_error::Context,
	futures::lock::Mutex,
	std::{collections::HashMap, path::PathBuf, sync::Arc},
	tokio::sync::OnceCell,
	zsw_util::AppError,
};

/// Profiles
#[derive(Debug)]
pub struct Profiles {
	/// Profiles directory
	root: PathBuf,

	/// Loaded profiles
	// TODO: Limit the size of this?
	profiles: Mutex<HashMap<ProfileName, Arc<OnceCell<Arc<Profile>>>>>,
}

impl Profiles {
	/// Creates a new profiles container
	pub fn new(root: PathBuf) -> Self {
		Self {
			root,
			profiles: Mutex::new(HashMap::new()),
		}
	}

	/// Loads a profile by name
	pub async fn load(&self, profile_name: ProfileName) -> Result<Arc<Profile>, AppError> {
		let profile_entry = Arc::clone(
			self.profiles
				.lock()
				.await
				.entry(profile_name.clone())
				.or_insert_with(|| Arc::new(OnceCell::new())),
		);

		profile_entry
			.get_or_try_init(async move || {
				// Try to read the file
				let profile_path = self.path_of(&profile_name);
				tracing::debug!("Loading profile {profile_name:?} from {profile_path:?}");
				let profile_toml = tokio::fs::read_to_string(profile_path)
					.await
					.context("Unable to open file")?;

				// And parse it
				let profile = toml::from_str::<ser::Profile>(&profile_toml).context("Unable to parse profile")?;
				let profile = Profile {
					panels: profile
						.panels
						.into_iter()
						.map(|panel| ProfilePanel {
							name:   PanelName::from(panel.name),
							shader: match panel.shader {
								ser::ProfilePanelShader::None(shader) =>
									ProfilePanelShader::None(ProfilePanelNoneShader {
										background_color: shader.background_color,
									}),
								ser::ProfilePanelShader::Fade(shader) =>
									ProfilePanelShader::Fade(ProfilePanelFadeShader {
										playlists:     shader.playlists.into_iter().map(PlaylistName::from).collect(),
										duration:      shader.duration,
										fade_duration: shader.fade_duration,
										inner:         match shader.inner {
											ser::ProfilePanelFadeShaderInner::Basic =>
												ProfilePanelFadeShaderInner::Basic,
											ser::ProfilePanelFadeShaderInner::White { strength } =>
												ProfilePanelFadeShaderInner::White { strength },
											ser::ProfilePanelFadeShaderInner::Out { strength } =>
												ProfilePanelFadeShaderInner::Out { strength },
											ser::ProfilePanelFadeShaderInner::In { strength } =>
												ProfilePanelFadeShaderInner::In { strength },
										},
									}),
							},
						})
						.collect(),
				};
				tracing::info!("Loaded profile {profile_name:?}");

				Ok(Arc::new(profile))
			})
			.await
			.map(Arc::clone)
	}

	/// Returns a profile's path
	pub fn path_of(&self, name: &ProfileName) -> PathBuf {
		self.root.join(&*name.0).with_added_extension("toml")
	}
}
