//! Uniforms
#define_import_path uniforms

/// Uniforms for each image
struct ImageUniforms {
	image_ratio: vec2<f32>,
	swap_dir: u32,
}

/// Uniforms
struct Uniforms {
	pos_matrix: mat4x4<f32>,
	prev: ImageUniforms,
	cur: ImageUniforms,
	next: ImageUniforms,

#ifdef SHADER_FADE
	fade_point: f32,
	progress: f32,

	#ifdef SHADER_FADE_WHITE
		strength: f32,
	#else ifdef SHADER_FADE_OUT
		strength: f32,
	#else ifdef SHADER_FADE_IN
		strength: f32,
	#endif
#endif
};

// Uniforms
@group(0) @binding(0)
var<uniform> uniforms: Uniforms;
