//! None shader

/// Uniforms
struct Uniforms {
	pos_matrix: mat4x4<f32>,
};

// Uniforms
@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@vertex
fn vs_main(@location(0) pos: vec2<f32>) -> @builtin(position) vec4<f32> {
	return uniforms.pos_matrix * vec4<f32>(pos, 0.0, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
	return vec4(0.0, 0.0, 0.0, 0.0);
}
