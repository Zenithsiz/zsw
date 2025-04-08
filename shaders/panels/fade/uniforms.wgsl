//! Uniforms
#include_once

/// Uniforms for each image
struct ImageUniforms {
	image_ratio: vec2<f32>,
	swap_dir: u32,
}

/// Uniforms
struct Uniforms {
	#match SHADER
	#match_case "none"
		pos_matrix: mat4x4<f32>,

	#match_case "fade"
		pos_matrix: mat4x4<f32>,
		prev: ImageUniforms,
		cur: ImageUniforms,
		next: ImageUniforms,
		fade_point: f32,
		progress: f32,

		#match SHADER_FADE_TYPE
		#match_case    "white"
		#match_case_or "out"
		#match_case_or "in"
			strength: f32,

		#match_end
	#match_end
};

// Uniforms
@group(0) @binding(0)
var<uniform> uniforms: Uniforms;
