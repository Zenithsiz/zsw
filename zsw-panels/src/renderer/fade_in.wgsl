
// Vertex input
struct VertexInput {
	@location(0)
	pos: vec2<f32>,

	@location(1)
	uvs: vec2<f32>,
};

// Vertex output
struct VertexOutput {
	@builtin(position)
	pos: vec4<f32>,

	@location(0)
	uvs: vec2<f32>,
};

// Uniforms
struct Uniforms {
	pos_matrix: mat4x4<f32>,
	uvs_matrix: mat4x4<f32>,
	alpha: f32,
	strength: f32,
};

// Uniforms
@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
	var out: VertexOutput;

	out.pos = uniforms.pos_matrix * vec4<f32>(in.pos, 0.0, 1.0);
	out.uvs = in.uvs;

	return out;
}

// Frag output
struct FragOutput {
	@location(0)
	color: vec4<f32>,
};

// Texture
@group(1) @binding(0)
var texture: texture_2d<f32>;

// Sampler
@group(1) @binding(1)
var texture_sampler: sampler;

@fragment
fn fs_main(in: VertexOutput) -> FragOutput {
	var out: FragOutput;

	// Sample the color and set the alpha
	let uvs = uniforms.uvs_matrix * vec4<f32>(in.uvs, 0.0, 1.0);
	let mid = vec2<f32>(uniforms.uvs_matrix[0][0] / 2.0 + uniforms.uvs_matrix[3].x, uniforms.uvs_matrix[1][1] / 2.0 + uniforms.uvs_matrix[3].y);
	let new_uvs = (uvs.xy - mid) / pow(uniforms.alpha, uniforms.strength) + mid;
	out.color = textureSample(texture, texture_sampler, new_uvs);
	out.color.a = uniforms.alpha;

	// TODO: Use a background color?
	if (new_uvs.x < 0.0 || new_uvs.x >= 1.0 || new_uvs.y < 0.0 || new_uvs.y >= 1.0) {
		out.color.a = 0.0;
	}

	return out;
}