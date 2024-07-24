//! Text-preprocessor

// Imports
use {
	anyhow::Context,
	itertools::Itertools,
	std::{
		borrow::Cow,
		collections::HashMap,
		fs,
		io::{BufRead, BufReader},
		path::{Path, PathBuf},
	},
};

/// Text preprocessor
#[derive(Debug)]
pub struct Tpp {
	/// Bindings
	bindings: HashMap<String, String>,

	/// Processed files (by canonicalized path)
	// TODO: Invalidate these when bindings change?
	processed_files: HashMap<PathBuf, ProcessedFile>,
}

impl Tpp {
	/// Creates a new text preprocessor
	#[must_use]
	pub fn new() -> Self {
		Self {
			bindings:        HashMap::new(),
			processed_files: HashMap::new(),
		}
	}

	/// Defines a value
	pub fn define(&mut self, binding: impl Into<String>, value: impl Into<String>) {
		#[expect(let_underscore_drop)] // We don't care if the binding already had a value
		let _ = self.bindings.insert(binding.into(), value.into());
	}

	/// Undefines a value
	pub fn undef(&mut self, binding: impl AsRef<str>) {
		// TODO: Should we care about it?
		#[expect(let_underscore_drop)] // We don't care if the binding already had a value
		let _ = self.bindings.remove(binding.as_ref());
	}

	/// Processes a file
	// TODO: Don't clone result once the borrow checker accepts the early return
	pub fn process(&mut self, path: impl AsRef<Path>) -> Result<String, anyhow::Error> {
		// Open the file and check if we already processed it
		let path = path.as_ref().canonicalize().context("Unable to canonicalize file")?;
		tracing::trace!(?path, "Processing file");
		if let Some(processed_file) = self.processed_files.get(&path) {
			tracing::trace!(?path, "File was already processed");
			match processed_file.include_once {
				true => {
					tracing::trace!("Not re-including file due to `#include_once`");
					return Ok(String::new());
				},
				false => return Ok(processed_file.contents.clone()),
			}
		}

		// Read all lines
		let file = fs::File::open(&path).context("Unable to open file")?;
		let file = BufReader::new(file);
		let lines = file
			.lines()
			.collect::<Result<Vec<_>, _>>()
			.context("Unable to read file lines")?;

		// Then parse
		let mut cur_state = State::Root { include_once: false };
		let contents = lines
			.iter()
			.enumerate()
			.batching(|lines| {
				// Get the next line
				let (line_idx, line) = lines.next()?;

				match self.process_line(&mut cur_state, &path, line) {
					Ok(Some(line)) => Some(Ok(Some(line))),
					Ok(None) => Some(Ok(None)),
					Err(err) => Some(Err(err).with_context(|| format!("Error on {}:{line_idx}", path.display()))),
				}
			})
			.flatten_ok()
			.collect::<Result<Vec<_>, anyhow::Error>>()?
			.into_iter()
			.join("\n");

		// If the final state wasn't root, return Err
		let include_once = match cur_state {
			State::Root { include_once } => include_once,
			State::Matching { .. } => anyhow::bail!("Missing `#match_end`"),
			State::Poisoned => unreachable!("State was poisoned"),
		};

		// Finally add it to the parsed files
		let processed_file = ProcessedFile { contents, include_once };
		tracing::trace!(?path, ?processed_file, "Processed file");
		let processed_file = self.processed_files.entry(path).insert_entry(processed_file).into_mut();
		Ok(processed_file.contents.clone())
	}

	/// Processes a line
	fn process_line<'line>(
		&mut self,
		cur_state: &mut State,
		cur_path: &Path,
		line: &'line str,
	) -> Result<Option<Cow<'line, str>>, anyhow::Error> {
		match cur_state {
			// `#include`
			State::Root { .. } if let Some(include_path_rel) = line.trim().strip_prefix("#include ") => {
				let include_path_rel = Self::parse_quoted_string(include_path_rel)?;

				// Then join it with the path's directory
				let include_path = cur_path
					.parent()
					.expect("File must have parent directory")
					.join(include_path_rel);
				let include_contents = self
					.process(&include_path)
					.with_context(|| format!("Unable to process included file {include_path:?}"))?;
				Ok(Some(include_contents.into()))
			},

			// `#include_once`
			State::Root { include_once } if let Some(rest) = line.trim().strip_prefix("#include_once") => {
				anyhow::ensure!(
					rest.trim().is_empty(),
					"Unexpected tokens after `#include_once`: {rest:?}"
				);
				match include_once {
					true => anyhow::bail!("Cannot specify `#include_once` twice"),
					false => *include_once = true,
				}

				Ok(None)
			},

			// `#match`
			State::Root { .. } if let Some(binding) = line.trim().strip_prefix("#match ") => {
				let binding = binding.trim();
				*cur_state = State::Matching {
					value:       self
						.bindings
						.get(binding)
						.with_context(|| format!("Unknown binding {binding:?}"))?
						.clone(),
					is_matching: None,
					prev_state:  Box::new(cur_state.replace_poisoned()),
				};

				Ok(None)
			},

			// `#match_case`
			State::Matching { value, is_matching, .. }
				if let Some(match_value) = line.trim().strip_prefix("#match_case ") =>
			{
				let match_value = Self::parse_quoted_string(match_value)?;
				*is_matching = Some(value == match_value);

				Ok(None)
			},

			// `#match_case_or`
			State::Matching { value, is_matching, .. }
				if let Some(match_value) = line.trim().strip_prefix("#match_case_or ") =>
			{
				let match_value = Self::parse_quoted_string(match_value)?;
				match is_matching {
					Some(is_matching) => *is_matching |= value == match_value,
					None => anyhow::bail!("Cannot use `#match_case_or` without a previous `#match_case`"),
				}

				Ok(None)
			},

			// `#match_end`
			State::Matching { prev_state, .. } if let Some(rest) = line.trim().strip_prefix("#match_end") => {
				anyhow::ensure!(rest.trim().is_empty(), "Unexpected tokens after `#match_end`: {rest:?}");
				*cur_state = prev_state.replace_poisoned();

				Ok(None)
			},

			// Inside `#match`
			State::Matching { is_matching, .. } => match is_matching {
				Some(true) => Ok(Some(line.into())),
				Some(false) => Ok(None),
				None => match line.trim().is_empty() {
					true => Ok(None),
					false => anyhow::bail!("Cannot parse code in `#match` without a `#match_case`"),
				},
			},

			// Unknown directive
			State::Root { .. } if let Some(unknown) = line.trim().strip_prefix('#') => {
				anyhow::bail!("Unknown directive: #{unknown}");
			},

			// Non-directive
			State::Root { .. } => Ok(Some(line.into())),

			State::Poisoned => unreachable!("State was poisoned"),
		}
	}

	/// Parses a quote-delimited string
	fn parse_quoted_string(s: &str) -> Result<&str, anyhow::Error> {
		s.trim()
			.strip_prefix('"')
			.and_then(|s| s.strip_suffix('"'))
			.with_context(|| format!("Expected quote delimited string, found {s:?}"))
	}
}

impl Default for Tpp {
	fn default() -> Self {
		Self::new()
	}
}

/// Processed file
#[derive(Debug)]
struct ProcessedFile {
	/// Contents
	contents: String,

	/// If the file should only be included once
	include_once: bool,
}

/// Processor state
#[derive(Debug)]
enum State {
	Root {
		include_once: bool,
	},
	Matching {
		value:       String,
		is_matching: Option<bool>,
		prev_state:  Box<Self>,
	},

	/// Poisoned.
	///
	/// Implies that a panic occurred while changing states
	Poisoned,
}

impl State {
	/// Replaces this state with a poisoned state
	pub fn replace_poisoned(&mut self) -> Self {
		std::mem::replace(self, Self::Poisoned)
	}
}
