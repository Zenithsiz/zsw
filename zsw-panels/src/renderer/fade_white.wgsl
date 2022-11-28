
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
	out.color = textureSample(texture, texture_sampler, uvs.xy) - (pow(uniforms.alpha, uniforms.strength) - 1.0);
	out.color.a = uniforms.alpha;

	return out;
}
