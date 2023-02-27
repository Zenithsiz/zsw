//! Uniforms
#include_once

/// Uniforms for each image
struct ImageUniforms {
	image_ratio: vec2<f32>,
	progress: f32,
	parallax_ratio: vec2<f32>,
	parallax_offset: vec2<f32>,
}

/// Uniforms
struct Uniforms {
	pos_matrix: mat4x4<f32>,
	front: ImageUniforms,
	back: ImageUniforms,
	front_alpha: f32,

	// Shader specific uniforms
	#match SHADER
	#match_case    "none"
	#match_case_or "fade"
		// Empty

	#match_case    "fade-white"
	#match_case_or "fade-out"
	#match_case_or "fade-in"
		strength: f32,

	#match_end
};

// Uniforms
@group(0) @binding(0)
var<uniform> uniforms: Uniforms;
