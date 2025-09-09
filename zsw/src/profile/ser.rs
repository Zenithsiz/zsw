//! Serialized profile

/// Profile
#[derive(Debug)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Profile {
	pub panels: Vec<String>,
}
