//! Client asset paths. Temporary filesystem lookup for development.

use std::path::PathBuf;

/// Development path to the Connection Frontend logo.
///
/// Packaged builds should change this function only — not frontend paint code.
#[must_use]
pub fn connection_logo_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Graphic/LOGO.png")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_logo_path_points_at_graphic_logo() {
        let path = connection_logo_path();
        assert!(
            path.ends_with("Graphic/LOGO.png") || path.ends_with("Graphic\\LOGO.png"),
            "{}",
            path.display()
        );
    }
}
