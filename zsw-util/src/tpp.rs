//! Text-preprocessor

// Imports
use {
	anyhow::Context,
	std::{
		collections::HashMap,
		fs,
		io::{BufRead, BufReader},
		path::Path,
	},
};

/// Text preprocessor
#[derive(Clone, Debug)]
pub struct Tpp {
	/// Bindings
	bindings: HashMap<String, String>,
}

impl Tpp {
	/// Creates a new text preprocessor
	#[must_use]
	pub fn new() -> Self {
		Self {
			bindings: HashMap::new(),
		}
	}

	/// Defines a value
	pub fn define(&mut self, binding: impl Into<String>, value: impl Into<String>) {
		#[allow(let_underscore_drop)] // We don't care if the binding already had a value
		let _ = self.bindings.insert(binding.into(), value.into());
	}

	/// Undefines a value
	pub fn undef(&mut self, binding: impl AsRef<str>) {
		// TODO: Should we care about it?
		#[allow(let_underscore_drop)] // We don't care if the binding already had a value
		let _ = self.bindings.remove(binding.as_ref());
	}

	/// Processes a file from it's path
	// TODO: Refactor?
	pub fn process(&mut self, path: impl AsRef<Path>) -> Result<String, anyhow::Error> {
		// Open the file
		let path = path.as_ref();
		let file = fs::File::open(path).context("Unable to open file")?;
		let file = BufReader::new(file);

		// Parser state:
		// TODO: Verify `if_stack` is empty by the end?
		let mut if_stack = vec![];

		// Parse all directives
		let lines = file.lines()
			.map(|line| -> Result<_, anyhow::Error> {
				// Get the line and append a newline to it, since `lines` removes newlines
				let mut line = line.context("Unable to read line")?;
				line.push('\n');

				// Parse any conditional code before-hand
				// TODO: Not special case this?
				match line {
					// `#ifdef`
					ref line if let Some(binding) = line.trim_start().strip_prefix("#ifdef ") => {
						let binding = binding.trim();

						let success = self.bindings.contains_key(binding);
						if_stack.push(IfBranch { state: IfState::If, success });
						return Ok(String::new());
					},

					// `#elifdef`
					ref line if let Some(binding) = line.trim_start().strip_prefix("#elifdef") => {
						let binding = binding.trim();

						let if_branch = if_stack.last_mut().context("Cannot use `#elifdef` before `#ifdef`")?;
						anyhow::ensure!(matches!(if_branch.state, IfState::If), "Cannot use `#elifdef` after `#else`");
						if_branch.success = !if_branch.success && self.bindings.contains_key(binding);
						return Ok(String::new());
					},

					// `#else`
					ref line if let Some(remainder) = line.trim_start().strip_prefix("#else") => {
						let remainder = remainder.trim();
						anyhow::ensure!(remainder.is_empty(), "Unexpected tokens after `#else`: {remainder:?}");

						let if_branch = if_stack.last_mut().context("Cannot use `#else` before `#ifdef`")?;
						anyhow::ensure!(matches!(if_branch.state, IfState::If), "Cannot use `#else` twice");
						if_branch.state = IfState::Else;
						return Ok(String::new());
					},

					// `#endif`
					ref line if let Some(remainder) = line.trim_start().strip_prefix("#endif") => {
						let remainder = remainder.trim();
						anyhow::ensure!(remainder.is_empty(), "Unexpected tokens after `#endif`: {remainder:?}");

						let _ = if_stack.pop().context("Cannot use `#endif` before `#ifdef`")?;
						return Ok(String::new());
					},

					// Else if there's an if-branch and we shouldn't continue, skip the line
					_ if let Some(if_branch) = if_stack.last() &&
						matches!((if_branch.state, if_branch.success), (IfState::If, false) | (IfState::Else, true)) => return Ok(String::new()),

					// Else continue
					_ => (),
				}

				// Then parse the line properly
				match line {
					// If we're including a file, process it too
					ref line if let Some(include_path_rel) = line.trim_start().strip_prefix("#include ") => {
						// Trim any whitespace on the filename
						let include_path_rel = include_path_rel.trim();

						// Then join it with the path's directory
						let include_path = path
							.parent()
							.expect("File must have parent directory")
							.join(include_path_rel);
						let include_contents = self
							.process(&include_path)
							.with_context(|| format!("Unable to process included file {include_path:?}"))?;
						Ok(include_contents)
					},

					// If it's an unknown directive, return Err
					ref line if let Some(directive) = line.trim_start().strip_prefix('#') => anyhow::bail!("Unknown directive: {directive:#}"),

					// Else just keep it verbatim (line feeds and all)
					line => Ok(line),
				}
			});

		// Then add context to all errors with the path + line
		let lines = lines
			.enumerate()
			.map(|(line_idx, res)| res.with_context(|| format!("Error on {}:{}", path.display(), line_idx + 1)));

		// Finally join all strings
		let lines = lines.collect::<Result<_, anyhow::Error>>();

		// Ensure we have no left-over `#ifdef` opens
		anyhow::ensure!(if_stack.is_empty(), "Unclosed `#ifdef`");

		lines
	}
}

impl Default for Tpp {
	fn default() -> Self {
		Self::new()
	}
}

/// Current parser position inside of an `#ifdef/#else/#endif` state
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum IfState {
	If,
	Else,
}

/// Current parser state inside of an `#ifdef/#else/#endif` state
#[derive(Clone, Copy, Debug)]
struct IfBranch {
	/// State
	state: IfState,

	/// If successful
	success: bool,
}
