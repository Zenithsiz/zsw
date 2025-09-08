//! Frag shader

// Imports
#import fade::stage_io::{VertexOutput, FragOutput}
#import fade::uniforms::{uniforms, ImageUniforms}

// Bindings
@group(1) @binding(0) var texture_prev: texture_2d<f32>;
@group(1) @binding(1) var texture_cur: texture_2d<f32>;
@group(1) @binding(2) var texture_next: texture_2d<f32>;
@group(1) @binding(3) var texture_sampler: sampler;

struct Sampled {
	color: vec4<f32>,
	uvs  : vec2<f32>,
}

// Samples a texture
fn sample(texture: texture_2d<f32>, in_uvs: vec2<f32>, image_uniforms: ImageUniforms, progress_raw: f32, alpha: f32) -> Sampled {
	var sampled: Sampled;
	var uvs = in_uvs;

	var progress: f32;
	if image_uniforms.swap_dir == 0 {
		progress = progress_raw;
	} else {
		progress = 1.0 - progress_raw;
	}

	// Then apply the image ratio and delta
	let uvs_delta = (vec2<f32>(1.0, 1.0) - image_uniforms.image_ratio) * progress;
	uvs = uvs * image_uniforms.image_ratio + uvs_delta;

	// Offset it, if necessary
	#ifdef FADE_OUT
		let mid = vec2<f32>(image_uniforms.image_ratio.x / 2.0 + uvs_delta.x, image_uniforms.image_ratio.y / 2.0 + uvs_delta.y);
		uvs = (uvs.xy - mid) * pow(alpha, uniforms.strength) + mid;

	#else ifdef FADE_IN
		let mid = vec2<f32>(image_uniforms.image_ratio.x / 2.0 + uvs_delta.x, image_uniforms.image_ratio.y / 2.0 + uvs_delta.y);
		uvs = (uvs.xy - mid) / pow(alpha, uniforms.strength) + mid;

	#endif

	sampled.color = textureSample(texture, texture_sampler, uvs);
	sampled.uvs = uvs;

	return sampled;
}

fn main(in: VertexOutput) -> FragOutput {
	var out: FragOutput;

	let p = uniforms.progress;
	let f = uniforms.fade_duration;

	// Full duration an image is on screen (including the fades)
	let d = 1.0 + 2.0 * f;

	let progress_prev = 1.0 - max((f - p) / d, 0.0);
	let progress_cur  = (p + f) / d;
	let progress_next = max((p - 1.0 + f) / d, 0.0);

	let alpha_prev = 0.5 * saturate(1.0 - (      p) / f);
	let alpha_next = 0.5 * saturate(1.0 - (1.0 - p) / f);
	let alpha_cur  = 1.0 - max(alpha_prev, alpha_next);

	// Sample the textures
	let sample_prev = sample(texture_prev, in.uvs, uniforms.prev, progress_prev, alpha_prev);
	let sample_cur  = sample( texture_cur, in.uvs, uniforms.cur , progress_cur , alpha_cur );
	let sample_next = sample(texture_next, in.uvs, uniforms.next, progress_next, alpha_next);

	// Then mix the color
	// TODO: Don't repeat this once we're able to use `defined(FADE_BASIC) || defined(FADE_OUT)`
	#ifdef FADE_BASIC
		out.color =
			alpha_prev * sample_prev.color +
			alpha_cur  * sample_cur .color +
			alpha_next * sample_next.color;
		out.color.a = 1.0;
	#else ifdef FADE_OUT
		out.color =
			alpha_prev * sample_prev.color +
			alpha_cur  * sample_cur .color +
			alpha_next * sample_next.color;
		out.color.a = 1.0;

	#else ifdef FADE_WHITE
		out.color =
			alpha_prev * sample_prev.color +
			alpha_cur  * sample_cur .color +
			alpha_next * sample_next.color;
		out.color = mix(
			out.color,
			vec4(1.0, 1.0, 1.0, 1.0),
			uniforms.strength * max(4.0 * alpha_cur * alpha_prev, 4.0 * alpha_cur * alpha_next)
		);
		out.color.a = 1.0;

	#else ifdef FADE_IN
		// TODO: Use a background color instead of black?
		let contained_prev = sample_prev.uvs.x >= 0.0 && sample_prev.uvs.x <= 1.0 && sample_prev.uvs.y >= 0.0 && sample_prev.uvs.y <= 1.0;
		let contained_cur  = sample_cur .uvs.x >= 0.0 && sample_cur .uvs.x <= 1.0 && sample_cur .uvs.y >= 0.0 && sample_cur .uvs.y <= 1.0;
		let contained_next = sample_next.uvs.x >= 0.0 && sample_next.uvs.x <= 1.0 && sample_next.uvs.y >= 0.0 && sample_next.uvs.y <= 1.0;
		out.color =
			alpha_prev * sample_prev.color * f32(contained_prev) +
			alpha_cur  * sample_cur .color * f32(contained_cur ) +
			alpha_next * sample_next.color * f32(contained_next) ;
		out.color.a = f32(contained_prev || contained_cur || contained_next);
	#endif

	return out;
}
