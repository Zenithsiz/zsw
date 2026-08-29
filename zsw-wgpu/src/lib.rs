//! Wgpu wrapper

#![feature(must_not_suspend, yeet_expr, share_trait)]

mod renderer;

pub use renderer::{FrameRender, WgpuRenderer};
