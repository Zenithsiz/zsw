//! Vertex shader
#include_once

// Includes
#include "stage_io.wgsl"
#include "uniforms.wgsl"

// Vertex entry
@vertex
fn vs_main(in: VertexInput) -> VertexOutputFragInput {
	var out: VertexOutputFragInput;

	out.pos = uniforms.pos_matrix * vec4<f32>(in.pos, 0.0, 1.0);
	out.uvs = in.uvs;

	return out;
}
