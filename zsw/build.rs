//! Build script

// Features
#![feature(must_not_suspend, path_add_extension)]

// Imports
use {
	app_error::{AppError, Context},
	itertools::Itertools,
	naga_oil::compose::{ComposableModuleDescriptor, Composer, ImportDefinition, NagaModuleDescriptor, ShaderDefValue},
	std::{
		collections::HashSet,
		env,
		fs,
		path::{Path, PathBuf},
	},
};

fn main() {
	let out_dir = env::var_os("OUT_DIR").expect("Missing `OUT_DIR`");
	let out_dir = PathBuf::from(out_dir);

	let shaders_out_dir = out_dir.join("shaders");

	// Pre-process all panel shaders
	let shaders = [
		PanelShader::None,
		PanelShader::Fade(PanelShaderFade::Basic),
		PanelShader::Fade(PanelShaderFade::White),
		PanelShader::Fade(PanelShaderFade::Out),
		PanelShader::Fade(PanelShaderFade::In),
	];
	for shader in shaders {
		let shader_src_path = Path::new("shaders").join(shader.src_path());
		let shader_out_path = shaders_out_dir.join(shader.out_path());
		if let Err(err) = shader.process(&shader_src_path, &shader_out_path) {
			panic!(
				"Unable to process shader module {shader_src_path:?} ({shader:?}): {}",
				err.pretty()
			)
		}
	}
}

/// Panel shader
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum PanelShader {
	None,
	Fade(PanelShaderFade),
}

/// Panel shader fade
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum PanelShaderFade {
	Basic,
	White,
	Out,
	In,
}


impl PanelShader {
	/// Returns this shader's source path, relative to the shaders path
	pub fn src_path(self) -> &'static str {
		match self {
			Self::None => "panels/none.wgsl",
			Self::Fade(_) => "panels/fade.wgsl",
		}
	}

	/// Returns this shader's output directory, relative to the shaders path
	pub fn out_path(self) -> &'static str {
		match self {
			Self::None => "panels/none.json",
			Self::Fade(fade) => match fade {
				PanelShaderFade::Basic => "panels/fade.json",
				PanelShaderFade::White => "panels/fade-white.json",
				PanelShaderFade::Out => "panels/fade-out.json",
				PanelShaderFade::In => "panels/fade-in.json",
			},
		}
	}

	/// Processes this shader from it's source and output
	fn process(self, shader_src_path: &Path, shader_out_path: &Path) -> Result<(), AppError> {
		// Parse the module itself
		let module = self.parse(shader_src_path).context("Unable to parse shader")?;

		// Then serialize and write it to the output path
		let module = serde_json::to_string(&module).context("Unable to serialize shader")?;
		fs::create_dir_all(shader_out_path.parent().context("Output path had no parent")?)
			.context("Unable to create output parent directory")?;
		fs::write(shader_out_path, &module).context("Unable to write shader")?;

		Ok(())
	}

	/// Parses this shader from it's source
	fn parse(self, shader_src_path: &Path) -> Result<naga::Module, AppError> {
		// Read the initial shader
		let shader_src_dir = shader_src_path
			.parent()
			.context("Shader path had no parent directory")?;
		let shader_src_path = shader_src_path
			.as_os_str()
			.to_str()
			.context("Shader path must be UTF-8")?;
		let shader_source = fs::read_to_string(shader_src_path).context("Unable to read shader file")?;

		// Import all modules that we need, starting with the main file and recursively
		// getting them all
		let mut composer = Composer::default();
		let (_, required_modules, _) = naga_oil::compose::get_preprocessor_data(&shader_source);
		for module in required_modules {
			self.parse_shader_module(shader_src_dir, &mut composer, &module)
				.with_context(|| format!("Unable to build import {:?}", module.import))?;
		}

		// Add any definitions required by the shader
		let mut shader_defs = HashSet::new();
		match self {
			Self::None => (),
			Self::Fade(fade) => match fade {
				PanelShaderFade::Basic => _ = shader_defs.insert("FADE_BASIC"),
				PanelShaderFade::White => _ = shader_defs.insert("FADE_WHITE"),
				PanelShaderFade::Out => _ = shader_defs.insert("FADE_OUT"),
				PanelShaderFade::In => _ = shader_defs.insert("FADE_IN"),
			},
		}

		// And finally build the final module.
		let shader_module = composer
			.make_naga_module(NagaModuleDescriptor {
				source: &shader_source,
				file_path: shader_src_path,
				shader_type: naga_oil::compose::ShaderType::Wgsl,
				shader_defs: shader_defs
					.into_iter()
					.map(|def| (def.to_owned(), ShaderDefValue::Bool(true)))
					.collect(),
				..Default::default()
			})
			.context("Unable to make naga module")?;

		Ok(shader_module)
	}

	/// Parses a shader module
	#[expect(clippy::only_used_in_recursion, reason = "We might need the shader in the future")]
	fn parse_shader_module(
		self,
		shader_src_dir: &Path,
		composer: &mut Composer,
		module: &ImportDefinition,
	) -> Result<(), AppError> {
		// If we already have the module, continue
		if composer.contains_module(&module.import) {
			return Ok(());
		}

		// Else read the module
		let module_path_rel = module.import.split("::").join("/");
		let module_path = shader_src_dir.join(&module_path_rel).with_extension("wgsl");
		let module_path = module_path.to_str().context("Module file name was non-utf8")?;
		let module_source = fs::read_to_string(module_path).context("Unable to read module file")?;

		// And get all required imports
		let (_, required_modules, _) = naga_oil::compose::get_preprocessor_data(&module_source);
		for module in required_modules {
			self.parse_shader_module(shader_src_dir, composer, &module)
				.with_context(|| format!("Unable to build import {:?}", module.import))?;
		}

		// Then add it as a module
		_ = composer
			.add_composable_module(ComposableModuleDescriptor {
				source: &module_source,
				file_path: module_path,
				language: naga_oil::compose::ShaderLanguage::Wgsl,
				as_name: Some(module.import.clone()),
				..Default::default()
			})
			.context("Unable to parse module")?;

		Ok(())
	}
}
