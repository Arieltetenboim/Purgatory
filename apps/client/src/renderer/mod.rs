mod camera;
mod gpu;
mod parallax;

#[allow(unused_imports)] // re-exported for tests / compact-stage callers
pub use camera::DEFAULT_LOGICAL_HEIGHT;
#[allow(unused_imports)]
pub use camera::clamp_camera_center;
pub use camera::{Camera, FOOTNOTE_LOGICAL_HEIGHT, is_usable_surface};
pub use gpu::{DrawQuad, FrameStatus, MAX_QUADS, OverlayPass, Renderer};
pub use parallax::{
    PARALLAX_FAR, PARALLAX_MID, PARALLAX_NEAR, parallax_debug_quads, parallax_quads,
};
