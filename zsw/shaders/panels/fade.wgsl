//! None shader

/// Uniforms
struct Uniforms {
	pos_matrix: mat4x4<f32>,
	image_ratio: vec2<f32>,
	progress: f32,
	alpha: f32,

	#ifdef FADE_WHITE
		mix_strength: f32,
	#else ifdef FADE_OUT
		strength: f32,
	#else ifdef FADE_IN
		strength: f32,
	#endif
};

/// Uniforms
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
	@location(1) uvs: vec2<f32>,
) -> VertexOutput {
	var out: VertexOutput;
	out.pos = uniforms.pos_matrix * vec4<f32>(pos, 0.0, 1.0);
	out.uvs = uvs;
	return out;
}

// Image
@group(1) @binding(0) var image: texture_2d<f32>;
@group(1) @binding(1) var image_sampler: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
	// Calculate the uvs for this pixel
	let uvs_offset = (vec2(1.0, 1.0) - uniforms.image_ratio) * uniforms.progress;
	var uvs = in.uvs * uniforms.image_ratio + uvs_offset;

	#ifdef FADE_OUT
		let mid = uniforms.image_ratio / 2.0 + uvs_offset;
		uvs = mid + (uvs - mid) * pow(uniforms.alpha, uniforms.strength);
	#else ifdef FADE_IN
		let mid = uniforms.image_ratio / 2.0 + uvs_offset;
		uvs = mid + (uvs - mid) / pow(uniforms.alpha, uniforms.strength);
	#endif

	// If we'd sample outside the image, discard this pixel instead
	// TODO: Set alpha to 0 instead of discarding?
	if any(uvs < vec2(0.0, 0.0) | uvs > vec2(1.0, 1.0)) {
		discard;
	}

	// Otherwise, we'll sample and return the color
	var color = textureSample(image, image_sampler, uvs);
	#ifdef FADE_WHITE
		color = mix(
			color,
			vec4(1.0, 1.0, 1.0, 1.0),
			uniforms.mix_strength,
		);
	#endif
	return vec4(color.rgb, uniforms.alpha);
}
