//! None shader

/// Uniforms
struct Uniforms {
	pos_matrix: mat4x4<f32>,
	image_ratio: vec2<f32>,
	offset: vec2<f32>,
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
	@location(0) in_pos: vec2<f32>,
	@location(1) in_uvs: vec2<f32>
) -> VertexOutput {
	var pos = in_pos;
	var uvs = in_uvs;

	// TODO: These transformations should be done in the `pos_matrix` instead,
	//       once we can abstract it better.
	pos += vec2(1.0, 1.0);
	if uniforms.offset.x != 0.0 {
		pos *= vec2(uniforms.image_ratio.y / uniforms.image_ratio.x, 1.0);
	}
	if uniforms.offset.y != 0.0 {
		pos *= vec2(1.0, uniforms.image_ratio.x / uniforms.image_ratio.y);
	}
	pos -= vec2(1.0, 1.0);

	pos += uniforms.offset;

	if pos.x < -1.0 {
		uvs.x -= (pos.x + 1.0) / 2.0 * uniforms.image_ratio.x / uniforms.image_ratio.y;
		pos.x = -1.0;
	}
	if pos.x > 1.0 {
		uvs.x += (1.0 - pos.x) / 2.0 * uniforms.image_ratio.x / uniforms.image_ratio.y;
		pos.x = 1.0;
	}
	if pos.y < -1.0 {
		uvs.y -= (pos.y + 1.0) / 2.0 * uniforms.image_ratio.y / uniforms.image_ratio.x;
		pos.y = -1.0;
	}
	if pos.y > 1.0 {
		uvs.y += (1.0 - pos.y) / 2.0 * uniforms.image_ratio.y / uniforms.image_ratio.x;
		pos.y = 1.0;
	}

	var out: VertexOutput;
	out.pos = uniforms.pos_matrix * vec4<f32>(
		pos,
		0.0,
		1.0
	);
	out.uvs = uvs;
	return out;
}

// Image
@group(1) @binding(0) var image: texture_2d<f32>;
@group(1) @binding(1) var image_sampler: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
	let color = textureSample(image, image_sampler, in.uvs);
	return vec4(color.rgb, 1.0);
}
