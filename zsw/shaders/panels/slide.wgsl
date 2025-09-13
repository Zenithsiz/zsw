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
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
	let pink = vec3(0.75, 0.56, 0.56);
	let black = vec3(0.0, 0.0, 0.0);

	let size = 256;

	let is_x = i32(pos.x) % size < size/2;
	let is_y = i32(pos.y) % size < size/2;

	if (u32(is_x) ^ u32(is_y)) != 0 {
		return vec4(pink, 0.5);
	} else {
		return vec4(black, 0.5);
	}
}
