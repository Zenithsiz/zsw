//! Stage Input/Output

// Vertex input
struct VertexInput {
	@location(0)
	pos: vec2<f32>,

	@location(1)
	uvs: vec2<f32>,
};

// Vertex output / Frag Input
struct VertexOutputFragInput {
	@builtin(position)
	pos: vec4<f32>,

	@location(0)
	uvs: vec2<f32>,
};

// Frag output
struct FragOutput {
	@location(0)
	color: vec4<f32>,
};
