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
@group(1) @binding(0) var front_texture: texture_2d<f32>;
@group(1) @binding(1) var back_texture: texture_2d<f32>;
@group(1) @binding(2) var texture_sampler: sampler;

struct Sampled {
	color: vec4<f32>,
	uvs  : vec2<f32>,
}

// Samples a texture
fn sample(texture: texture_2d<f32>, uvs: vec2<f32>, image_uniforms: ImageUniforms, alpha: f32) -> Sampled {
	var sampled: Sampled;
	var uvs = uvs;

	// Apply parallax to the uvs first
	{
		let mid = vec2<f32>(0.5, 0.5);
		uvs = (uvs - mid) * image_uniforms.parallax_ratio + mid + image_uniforms.parallax_offset;
	}

	// Then apply the image ratio and delta
	let uvs_delta = (vec2<f32>(1.0, 1.0) - image_uniforms.image_ratio) * image_uniforms.progress;
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

	// Sample the textures
	let front_sample = sample(front_texture, in.uvs, uniforms.front,       uniforms.front_alpha);
	let  back_sample = sample( back_texture, in.uvs, uniforms. back, 1.0 - uniforms.front_alpha);

	// Then mix the color
	#match SHADER
	#match_case    "none"
		out.color = vec4(0.0);

	#match_case    "fade"
	#match_case_or "fade-out"
		out.color = mix(back_sample.color, front_sample.color, uniforms.front_alpha);
		out.color.a = 1.0;

	#match_case "fade-white"
		out.color = mix(back_sample.color, front_sample.color, uniforms.front_alpha) - (pow(uniforms.front_alpha, uniforms.strength) - 1.0);
		out.color.a = 1.0;

	#match_case "fade-in"
		// TODO: Use a background color instead of black?
		let front_contained = front_sample.uvs.x >= 0.0 && front_sample.uvs.x <= 1.0 && front_sample.uvs.y >= 0.0 && front_sample.uvs.y <= 1.0;
		let  back_contained =  back_sample.uvs.x >= 0.0 &&  back_sample.uvs.x <= 1.0 &&  back_sample.uvs.y >= 0.0 &&  back_sample.uvs.y <= 1.0;
		out.color = mix(back_sample.color * f32(back_contained), front_sample.color * f32(front_contained), uniforms.front_alpha);
		out.color.a = f32(front_contained || back_contained);
	#match_end

	return out;
}
