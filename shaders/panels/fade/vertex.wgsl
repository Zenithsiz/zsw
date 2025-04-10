//! Vertex shader

// Imports
#import fade::stage_io::{VertexInput, VertexOutputFragInput}
#import fade::uniforms::uniforms

// Vertex entry
fn main(in: VertexInput) -> VertexOutputFragInput {
	var out: VertexOutputFragInput;

	out.pos = uniforms.pos_matrix * vec4<f32>(in.pos, 0.0, 1.0);
	out.uvs = in.uvs;

	return out;
}
