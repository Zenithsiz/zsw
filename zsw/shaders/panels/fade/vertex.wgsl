//! Vertex shader

// Imports
#import fade::stage_io::{VertexInput, VertexOutput}
#import fade::uniforms::uniforms

// Vertex entry
fn main(in: VertexInput) -> VertexOutput {
	var out: VertexOutput;

	out.pos = uniforms.pos_matrix * vec4<f32>(in.pos, 0.0, 1.0);
	out.uvs = in.uvs;

	return out;
}
