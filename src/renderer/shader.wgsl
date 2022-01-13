
// Vertex input
struct VertexInput {
	[[location(0)]]
	pos: vec3<f32>;
	
	[[location(1)]]
	color: vec3<f32>;
};

// Vertex output
struct VertexOutput {
	[[builtin(position)]]
	clip_position: vec4<f32>;
	
	[[location(0)]]
	color: vec3<f32>;
};

[[stage(vertex)]]
fn vs_main(input: VertexInput) -> VertexOutput {
	var out: VertexOutput;
	
	out.clip_position = vec4<f32>(input.pos, 1.0);
	out.color = input.color;
	
	return out;
}

[[stage(fragment)]]
fn fs_main(input: VertexOutput) -> [[location(0)]] vec4<f32> {
	return vec4<f32>(input.color, 1.0);
}
