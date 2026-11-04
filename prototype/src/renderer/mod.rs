pub mod softbuffer_skia;
pub mod wgpu_renderer;

use crate::physics::WorldState;

/// Universal rendering backend interface.
/// Allows swapping between the CPU 2D software renderer (softbuffer + tiny-skia)
/// and a future GPU accelerated renderer (wgpu) without modifying physics or OS loop code.
pub trait RenderBackend {
    /// Resizes the internal rendering target and pixel buffers.
    fn resize(&mut self, width: u32, height: u32);

    /// Renders a single frame of the world state, including the pet, speech bubbles,
    /// and optional neon platform debug overlays.
    fn render_frame(&mut self, world: &WorldState, debug_draw_platforms: bool);
}
