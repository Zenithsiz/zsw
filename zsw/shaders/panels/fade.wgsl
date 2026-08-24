//! None shader

/// Image uniforms
struct ImageUniforms {
	image_ratio: vec2<f32>,
	progress: f32,
	alpha: f32,
}

/// Images uniforms
struct ImagesUniforms {
	prev: ImageUniforms,
	cur: ImageUniforms,
	next: ImageUniforms,
}

/// Uniforms
struct Uniforms {
	pos_matrix: mat4x4<f32>,
	images: ImagesUniforms,

	#ifdef FADE_OUT
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
@group(1) @binding(0) var prev_image: texture_2d<f32>;
@group(1) @binding(1) var cur_image: texture_2d<f32>;
@group(1) @binding(2) var next_image: texture_2d<f32>;
@group(1) @binding(3) var image_sampler: sampler;

fn image_sample(in_uvs: vec2<f32>, image: texture_2d<f32>, image_uniforms: ImageUniforms) -> vec4<f32> {
	// Calculate the uvs for this pixel
	let uvs_offset = (vec2(1.0, 1.0) - image_uniforms.image_ratio) * image_uniforms.progress;
	var uvs = in_uvs * image_uniforms.image_ratio + uvs_offset;

	#ifdef FADE_OUT
		let mid = image_uniforms.image_ratio / 2.0 + uvs_offset;
		uvs = mid + (uvs - mid) * pow(image_uniforms.alpha, uniforms.strength);
	#endif

	// If we'd sample outside the image, discard this pixel instead
	if uvs.x < 0.0 || uvs.y < 0.0 || uvs.x > 1.0 || uvs.y > 1.0 {
		return vec4(0.0);
	}

	// Otherwise, we'll sample and return the color
	let color = textureSample(image, image_sampler, uvs);
	return vec4(color.rgb, image_uniforms.alpha);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
	let prev_color = image_sample(in.uvs, prev_image, uniforms.images.prev);
	let cur_color = image_sample(in.uvs, cur_image, uniforms.images.cur);
	let next_color = image_sample(in.uvs, next_image, uniforms.images.next);

	var total_alpha = prev_color.a + cur_color.a + next_color.a;
	var color = (
		prev_color.rgb * prev_color.a +
		cur_color.rgb * cur_color.a +
		next_color.rgb * next_color.a
	) / total_alpha;
	return vec4(color, 1.0);
}
