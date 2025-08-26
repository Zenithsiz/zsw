//! Fade shader

// Imports
#import fade::vertex
#import fade::frag
#import fade::stage_io::{VertexInput, VertexOutput, FragOutput}


@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
	return vertex::main(in);
}

@fragment
fn fs_main(in: VertexOutput) -> FragOutput {
	return frag::main(in);
}
