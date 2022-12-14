
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

struct ImageUniforms {
	image_ratio: vec2<f32>,
	progress: f32,
	parallax_ratio: vec2<f32>,
	parallax_offset: vec2<f32>,
}

// Uniforms
struct Uniforms {
	pos_matrix: mat4x4<f32>,
	front: ImageUniforms,
	back: ImageUniforms,
	front_alpha: f32,
#ifdef FADE
#elifdef FADE_WHITE
	strength: f32,
#elifdef FADE_OUT
	strength: f32,
#elifdef FADE_IN
	strength: f32,
#endif
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

// Front texture
@group(1) @binding(0)
var front_texture: texture_2d<f32>;

// Back texture
@group(1) @binding(1)
var back_texture: texture_2d<f32>;

// Sampler
@group(1) @binding(2)
var texture_sampler: sampler;

struct Sampled {
	color: vec4<f32>,
	uvs  : vec2<f32>,
}

// Samples a texture
fn sample(texture: texture_2d<f32>, uvs: vec2<f32>, image_uniforms: ImageUniforms, alpha: f32) -> Sampled {
	var sampled: Sampled;

	// Apply parallax to the uvs first
	let mid = vec2<f32>(0.5, 0.5);
	let uvs = (uvs - mid) * image_uniforms.parallax_ratio + mid + image_uniforms.parallax_offset;

	// Then apply the image ratio and delta
	let uvs_delta = (vec2<f32>(1.0, 1.0) - image_uniforms.image_ratio) * image_uniforms.progress;
	let uvs = uvs * image_uniforms.image_ratio + uvs_delta;

	#ifdef FADE
		sampled.color = textureSample(texture, texture_sampler, uvs);
		sampled.uvs = uvs;
	#elifdef FADE_WHITE
		sampled.color = textureSample(texture, texture_sampler, uvs);
		sampled.uvs = uvs;
	#elifdef FADE_OUT
		let mid = vec2<f32>(image_uniforms.image_ratio.x / 2.0 + uvs_delta.x, image_uniforms.image_ratio.y / 2.0 + uvs_delta.y);
		let new_uvs = (uvs.xy - mid) * pow(alpha, uniforms.strength) + mid;
		sampled.color = textureSample(texture, texture_sampler, new_uvs);
		sampled.uvs = new_uvs;
	#elifdef FADE_IN
		let mid = vec2<f32>(image_uniforms.image_ratio.x / 2.0 + uvs_delta.x, image_uniforms.image_ratio.y / 2.0 + uvs_delta.y);
		let new_uvs = (uvs.xy - mid) / pow(alpha, uniforms.strength) + mid;
		sampled.color = textureSample(texture, texture_sampler, new_uvs);
		sampled.uvs = new_uvs;
	#endif

	return sampled;
}

@fragment
fn fs_main(in: VertexOutput) -> FragOutput {
	var out: FragOutput;

	// Sample the color
	let front_sample = sample(front_texture, in.uvs, uniforms.front,       uniforms.front_alpha);
	let  back_sample = sample( back_texture, in.uvs, uniforms. back, 1.0 - uniforms.front_alpha);

	#ifdef FADE
		out.color = mix(back_sample.color, front_sample.color, uniforms.front_alpha);
		out.color.a = 1.0;
	#elifdef FADE_WHITE
		out.color = mix(back_sample.color, front_sample.color, uniforms.front_alpha) - (pow(uniforms.front_alpha, uniforms.strength) - 1.0);
		out.color.a = 1.0;
	#elifdef FADE_OUT
		out.color = mix(back_sample.color, front_sample.color, uniforms.front_alpha);
		//out.color = front_sample.color;
		out.color.a = 1.0;
	#elifdef FADE_IN
		// TODO: Use a background color instead of black?
		let front_contained = front_sample.uvs.x >= 0.0 && front_sample.uvs.x <= 1.0 && front_sample.uvs.y >= 0.0 && front_sample.uvs.y <= 1.0;
		let  back_contained =  back_sample.uvs.x >= 0.0 &&  back_sample.uvs.x <= 1.0 &&  back_sample.uvs.y >= 0.0 &&  back_sample.uvs.y <= 1.0;
		out.color = mix(back_sample.color * f32(back_contained), front_sample.color * f32(front_contained), uniforms.front_alpha);
		out.color.a = 1.0;
	#endif

	return out;
}
