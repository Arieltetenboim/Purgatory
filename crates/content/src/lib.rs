//! Content definition loading and validation boundary.
//!
//! Server and tools consume definitions. Image textures are a client
//! presentation concern and must not be required to understand content data.

/// Cargo package version for this crate.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_nonempty() {
        assert!(!version().is_empty());
    }

    #[test]
    fn common_is_linked() {
        assert!(!purgatory_common::version().is_empty());
    }
}
