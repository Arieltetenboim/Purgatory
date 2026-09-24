//! PNG loading boundary for client assets; `AssetRuntime` remains the registry.

use std::path::{Component, Path, PathBuf};

use crate::asset_runtime::AssetRuntime;
use crate::renderer::SpriteTextureId;

/// Current source contract: the checkout's Graphic directory, independent of cwd.
/// Packaging is not defined yet; change this resolver when it is.
fn graphic_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Graphic")
}

/// Synchronous startup loader. Register resources before `Renderer::new` uploads
/// `AssetRuntime::resources()`; this does not upload textures to a live renderer.
///
/// Logical keys follow the registry's first-registration-wins contract, including
/// embedded registrations. Once registered, a key is returned without I/O or
/// decode, even if a different source path is supplied. Use a new key for a
/// different asset; this API does not replace or hot-reload existing resources.
#[cfg_attr(not(test), allow(dead_code))] // A0 proof is a focused fixture; consumers follow later.
pub(crate) struct ClientAssetLoader<'a> {
    runtime: &'a mut AssetRuntime,
    root: PathBuf,
}

#[cfg_attr(not(test), allow(dead_code))]
impl<'a> ClientAssetLoader<'a> {
    pub(crate) fn new(runtime: &'a mut AssetRuntime) -> Self {
        Self {
            runtime,
            root: graphic_root(),
        }
    }

    /// `path` is relative to Graphic/, e.g. `frontend_scene_guide.png`.
    /// Keys are nonempty ASCII letters/digits plus `.`, `_`, or `-`.
    /// Absolute paths and traversal components are rejected.
    pub(crate) fn load_png(
        &mut self,
        key: &str,
        path: impl AsRef<Path>,
    ) -> Result<SpriteTextureId, String> {
        if key.is_empty()
            || !key
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        {
            return Err(format!("invalid PNG asset key {key:?}"));
        }
        let relative = path.as_ref();
        if relative.as_os_str().is_empty()
            || relative
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err(format!(
                "invalid PNG asset path for {key}: {}",
                relative.display()
            ));
        }
        if let Some(texture) = self.runtime.texture_for_key(key) {
            return Ok(texture);
        }
        let path = self.root.join(relative);
        let bytes = std::fs::read(&path).map_err(|error| {
            let operation = if error.kind() == std::io::ErrorKind::NotFound {
                "missing PNG asset"
            } else {
                "read PNG asset"
            };
            format!("{operation} {key} at {}: {error}", path.display())
        })?;
        let image = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
            .map_err(|error| format!("decode PNG asset {key} at {}: {error}", path.display()))?
            .to_rgba8();
        self.runtime.register_image(key, image)
    }
}

/// Development path to the Connection Frontend logo, sharing the source root.
#[must_use]
pub fn connection_logo_path() -> PathBuf {
    graphic_root().join("LOGO.png")
}

/// Bundled production UI face (DejaVu Sans Bold); license accompanies the asset.
pub const UI_FONT: &[u8] = include_bytes!("../assets/fonts/DejaVuSans-Bold.ttf");

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "purgatory-asset-{}-{}",
                std::process::id(),
                NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(root.join("nested")).unwrap();
            image::RgbaImage::from_pixel(1, 1, image::Rgba([12, 34, 56, 255]))
                .save(root.join("nested/proof.png"))
                .unwrap();
            Self(root)
        }

        fn loader<'a>(&self, runtime: &'a mut AssetRuntime) -> ClientAssetLoader<'a> {
            ClientAssetLoader {
                runtime,
                root: self.0.clone(),
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn asset_png_fixture_registers_pixels_and_reuses_identity_without_io() {
        let fixture = Fixture::new();
        let mut runtime = AssetRuntime::new();
        let first = fixture
            .loader(&mut runtime)
            .load_png("proof.image", "nested/proof.png")
            .unwrap();
        std::fs::remove_file(fixture.0.join("nested/proof.png")).unwrap();
        let second = fixture
            .loader(&mut runtime)
            .load_png("proof.image", "nested/proof.png")
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(runtime.resource_count(), 1);
        let resource = runtime.resource(first).unwrap();
        assert_eq!(resource.image.dimensions(), (1, 1));
        assert_eq!(resource.image.get_pixel(0, 0).0, [12, 34, 56, 255]);
    }

    #[test]
    fn asset_keys_follow_existing_first_registration_wins_contract() {
        let fixture = Fixture::new();
        let mut runtime = AssetRuntime::new();
        let existing = runtime
            .register_image("proof.image", image::RgbaImage::new(2, 2))
            .unwrap();
        let loaded = fixture
            .loader(&mut runtime)
            .load_png("proof.image", "different.png")
            .unwrap();
        assert_eq!(loaded, existing);
        assert_eq!(runtime.resource_count(), 1);
        assert_eq!(runtime.resource(loaded).unwrap().image.dimensions(), (2, 2));
    }

    #[test]
    fn asset_missing_and_unreadable_sources_report_key_and_path() {
        let fixture = Fixture::new();
        let mut runtime = AssetRuntime::new();
        for (path, prefix) in [
            ("missing.png", "missing PNG asset"),
            ("nested", "read PNG asset"),
        ] {
            let error = fixture
                .loader(&mut runtime)
                .load_png("proof.image", path)
                .unwrap_err();
            assert!(
                error.starts_with(&format!("{prefix} proof.image at ")),
                "{error}"
            );
            assert!(
                error.contains(&fixture.0.join(path).display().to_string()),
                "{error}"
            );
        }
        assert_eq!(runtime.resource_count(), 0);
    }

    #[test]
    fn asset_invalid_png_is_not_registered_and_can_be_retried() {
        let fixture = Fixture::new();
        std::fs::write(fixture.0.join("bad.png"), b"not a PNG").unwrap();
        let mut runtime = AssetRuntime::new();
        let error = fixture
            .loader(&mut runtime)
            .load_png("proof.image", "bad.png")
            .unwrap_err();
        assert!(
            error.starts_with("decode PNG asset proof.image at "),
            "{error}"
        );
        assert!(error.contains(&fixture.0.join("bad.png").display().to_string()));
        assert_eq!(runtime.resource_count(), 0);
        fixture
            .loader(&mut runtime)
            .load_png("proof.image", "nested/proof.png")
            .unwrap();
        assert_eq!(runtime.resource_count(), 1);
    }

    #[test]
    fn asset_invalid_keys_and_paths_are_rejected() {
        let fixture = Fixture::new();
        let mut runtime = AssetRuntime::new();
        for key in ["", "has space", "absolute/key"] {
            assert!(
                fixture
                    .loader(&mut runtime)
                    .load_png(key, "nested/proof.png")
                    .unwrap_err()
                    .starts_with("invalid PNG asset key")
            );
        }
        for path in [
            Path::new(""),
            Path::new("../proof.png"),
            fixture.0.as_path(),
        ] {
            assert!(
                fixture
                    .loader(&mut runtime)
                    .load_png("proof.image", path)
                    .unwrap_err()
                    .starts_with("invalid PNG asset path")
            );
        }
        assert_eq!(runtime.resource_count(), 0);
    }

    #[test]
    fn asset_default_root_is_checkout_graphic_not_working_directory() {
        let mut runtime = AssetRuntime::new();
        let loader = ClientAssetLoader::new(&mut runtime);
        let checkout = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        assert!(loader.root.is_absolute());
        assert_eq!(
            loader.root.canonicalize().unwrap(),
            checkout.join("Graphic").canonicalize().unwrap()
        );
        assert_eq!(connection_logo_path(), loader.root.join("LOGO.png"));
    }
}
