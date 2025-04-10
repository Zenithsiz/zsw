//! Fade shader

// Imports
#import fade::vertex
#import fade::frag
#import fade::stage_io::{VertexInput, VertexOutputFragInput, FragOutput}


@vertex
fn vs_main(in: VertexInput) -> VertexOutputFragInput {
	return vertex::main(in);
}

@fragment
fn fs_main(in: VertexOutputFragInput) -> FragOutput {
	return frag::main(in);
}
