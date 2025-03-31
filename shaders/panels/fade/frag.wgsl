//! Frag shader
#include_once

// Includes
#include "stage_io.wgsl"
#include "uniforms.wgsl"

// Frag output
struct FragOutput {
	@location(0)
	color: vec4<f32>,
};

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
fn sample(texture: texture_2d<f32>, in_uvs: vec2<f32>, image_uniforms: ImageUniforms, progress: f32, alpha: f32) -> Sampled {
	var sampled: Sampled;
	var uvs = in_uvs;

	// Then apply the image ratio and delta
	let uvs_delta = (vec2<f32>(1.0, 1.0) - image_uniforms.image_ratio) * progress;
	uvs = uvs * image_uniforms.image_ratio + uvs_delta;

	// Offset it, if necessary
	{
	#match SHADER
	#match_case    "none"
	#match_case_or "fade"
	#match_case_or "fade-white"
		// Empty

	#match_case "fade-out"
		let mid = vec2<f32>(image_uniforms.image_ratio.x / 2.0 + uvs_delta.x, image_uniforms.image_ratio.y / 2.0 + uvs_delta.y);
		uvs = (uvs.xy - mid) * pow(alpha, uniforms.strength) + mid;

	#match_case "fade-in"
		let mid = vec2<f32>(image_uniforms.image_ratio.x / 2.0 + uvs_delta.x, image_uniforms.image_ratio.y / 2.0 + uvs_delta.y);
		uvs = (uvs.xy - mid) / pow(alpha, uniforms.strength) + mid;

	#match_end
	}

	sampled.color = textureSample(texture, texture_sampler, uvs);
	sampled.uvs = uvs;

	return sampled;
}

@fragment
fn fs_main(in: VertexOutputFragInput) -> FragOutput {
	var out: FragOutput;

	let progress_prev = 1.0 - max((1.0 - uniforms.progress) - uniforms.fade_point, 0.0);
	let progress_cur  = uniforms.progress;
	let progress_next = max(uniforms.progress - uniforms.fade_point, 0.0);

	let alpha_prev = max(((1.0 - progress_cur) - uniforms.fade_point) / (1.0 - uniforms.fade_point), 0.0);
	let alpha_next = max((progress_cur - uniforms.fade_point) / (1.0 - uniforms.fade_point), 0.0);
	let alpha_cur  = 1.0 - max(alpha_prev, alpha_next);

	// Sample the textures
	let sample_prev = sample(texture_prev, in.uvs, uniforms.prev, progress_prev, alpha_prev);
	let sample_cur  = sample( texture_cur, in.uvs, uniforms.cur , progress_cur , alpha_cur );
	let sample_next = sample(texture_next, in.uvs, uniforms.next, progress_next, alpha_next);

	// Then mix the color
	#match SHADER
	#match_case    "none"
		out.color = vec4(0.0);

	#match_case    "fade"
	#match_case_or "fade-out"
		out.color =
			alpha_prev * sample_prev.color +
			alpha_cur  * sample_cur .color +
			alpha_next * sample_next.color;
		out.color.a = 1.0;

	#match_case "fade-white"
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

	#match_case "fade-in"
		// TODO: Use a background color instead of black?
		let contained_prev = sample_prev.uvs.x >= 0.0 && sample_prev.uvs.x <= 1.0 && sample_prev.uvs.y >= 0.0 && sample_prev.uvs.y <= 1.0;
		let contained_cur  = sample_cur .uvs.x >= 0.0 && sample_cur .uvs.x <= 1.0 && sample_cur .uvs.y >= 0.0 && sample_cur .uvs.y <= 1.0;
		let contained_next = sample_next.uvs.x >= 0.0 && sample_next.uvs.x <= 1.0 && sample_next.uvs.y >= 0.0 && sample_next.uvs.y <= 1.0;
		out.color =
			alpha_prev * sample_prev.color * f32(contained_prev) +
			alpha_cur  * sample_cur .color * f32(contained_cur ) +
			alpha_next * sample_next.color * f32(contained_next) ;
		out.color.a = f32(contained_prev || contained_cur || contained_next);
	#match_end

	return out;
}
