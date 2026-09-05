//! Native window creation. Default size comes from [`crate::display::DisplaySettings`].

use winit::dpi::PhysicalSize;
use winit::window::{Window, WindowAttributes};

use crate::display::Resolution;

pub const DEV_WINDOW_WIDTH: u32 = Resolution::DEFAULT.width;
pub const DEV_WINDOW_HEIGHT: u32 = Resolution::DEFAULT.height;
pub const WINDOW_TITLE: &str = "PURGATORY — Engine Dev";

#[must_use]
pub fn window_title() -> String {
    format!(
        "{WINDOW_TITLE}  v{}  phase {}",
        purgatory_common::version(),
        purgatory_common::phase()
    )
}

#[must_use]
pub fn window_attributes() -> WindowAttributes {
    Window::default_attributes()
        .with_title(window_title())
        .with_inner_size(PhysicalSize::new(DEV_WINDOW_WIDTH, DEV_WINDOW_HEIGHT))
}

#[must_use]
pub fn diagnostic_title(width: u32, height: u32, tick: u64, frames: u64) -> String {
    format!(
        "{} | {width}x{height} | tick {tick} | frames {frames}",
        window_title()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn development_window_is_1280x720() {
        assert_eq!(DEV_WINDOW_WIDTH, 1280);
        assert_eq!(DEV_WINDOW_HEIGHT, 720);
    }
}
