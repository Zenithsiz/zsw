//! None shader

/// Uniforms
struct Uniforms {
	pos_matrix: mat4x4<f32>,
};

// Uniforms
@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

/// Vertex output
struct VertexOutput {
	@builtin(position)
	pos: vec4<f32>,

	@location(0)
	uvs: vec2<f32>,
};

@vertex
fn vs_main(
	@location(0) pos: vec2<f32>,
	@location(1) uvs: vec2<f32>
) -> VertexOutput {
	var out: VertexOutput;
	out.pos = uniforms.pos_matrix * vec4<f32>(pos, 0.0, 1.0);
	out.uvs = uvs;
	return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
	let pink = vec3(0.75, 0.56, 0.56);
	let black = vec3(0.0, 0.0, 0.0);

	let size = 256;

	let is_x = i32(in.pos.x) % size < size/2;
	let is_y = i32(in.pos.y) % size < size/2;

	if (u32(is_x) ^ u32(is_y)) != 0 {
		return vec4(pink, 0.5);
	} else {
		return vec4(black, 0.5);
	}
}
