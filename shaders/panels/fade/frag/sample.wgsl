//! Texture sampling
#include_once

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
