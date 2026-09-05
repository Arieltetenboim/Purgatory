mod camera;
mod gpu;
mod parallax;
pub(crate) mod rf_diag;

#[allow(unused_imports)] // re-exported for tests / compact-stage callers
pub use camera::DEFAULT_LOGICAL_HEIGHT;
#[allow(unused_imports)]
pub use camera::clamp_camera_center;
#[allow(unused_imports)] // re-exported for tests
pub use camera::is_usable_surface;
pub use camera::{Camera, FOOTNOTE_LOGICAL_HEIGHT};
#[allow(unused_imports)] // gameplay view helpers used by renderer internals / tests
pub use camera::{
    GAMEPLAY_ASPECT, PixelViewport, constrained_pixel_viewport, gameplay_viewport_size,
};
#[allow(unused_imports)] // OverlayPass is for DEV egui submit; shipping keeps the type
pub use gpu::OverlayPass;
#[allow(unused_imports)] // WorldMsaa is DEV Display-selected; shipping stays on the default
pub use gpu::{DrawQuad, FrameStatus, MAX_QUADS, Renderer, WorldMsaa};
#[allow(unused_imports)] // DEV overlay / diagnostics assemble; shipping may omit consumers
pub use parallax::{
    PARALLAX_FAR, PARALLAX_MID, PARALLAX_NEAR, parallax_debug_quads, parallax_quads,
};
