
// Vertex input
struct VertexInput {
	[[location(0)]]
	pos: vec2<f32>;
	
	[[location(1)]]
	uvs: vec2<f32>;
};

// Vertex output
struct VertexOutput {
	[[builtin(position)]]
	pos: vec4<f32>;
	
	[[location(0)]]
	uvs: vec2<f32>;
};

// Uniforms
struct Uniforms {
	matrix: mat4x4<f32>;
	uvs_offset: vec2<f32>;
	alpha: f32;
};

// Uniforms
[[group(0), binding(0)]]
var<uniform> uniforms: Uniforms;

[[stage(vertex)]]
fn vs_main(in: VertexInput) -> VertexOutput {
	var out: VertexOutput;
	
	out.pos = uniforms.matrix * vec4<f32>(in.pos, 0.0, 1.0);
	out.uvs = in.uvs;
	
	return out;
}

// Frag output
struct FragOutput {
	[[location(0)]]
	color: vec4<f32>;
};

// Texture
[[group(1), binding(0)]]
var texture: texture_2d<f32>;

// Sampler
[[group(1), binding(1)]]
var texture_sampler: sampler;

[[stage(fragment)]]
fn fs_main(in: VertexOutput) -> FragOutput {
	var out: FragOutput;

	// Sample the color and set the alpha
	out.color = textureSample(texture, texture_sampler, in.uvs + uniforms.uvs_offset);
	out.color.a = uniforms.alpha;

	return out;
}
