//! Vertex

/// Vertex
#[derive(Clone, Copy, Debug)]
pub struct Vertex {
	pub vertex_pos: [f32; 2],
	pub vertex_tex: [f32; 2],
}

glium::implement_vertex!(Vertex, vertex_pos, vertex_tex);
