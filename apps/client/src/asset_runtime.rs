//! Client-owned embedded sprite resources.
//!
//! This layer owns decoded resource data and stable client texture identities.
//! Domain code owns what a visual means; the renderer owns submission.

use std::collections::HashMap;

use image::RgbaImage;

use crate::renderer::SpriteTextureId;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ResolvedVisual {
    pub(crate) texture: SpriteTextureId,
    pub(crate) rect_px: [u32; 4],
    pub(crate) uv: [[f32; 2]; 4],
    pub(crate) pivot_px: [f32; 2],
    pub(crate) dimensions_px: [u32; 2],
    pub(crate) pixels_per_unit: f32,
}

#[derive(Debug)]
pub(crate) struct SpriteResource {
    pub(crate) id: SpriteTextureId,
    pub(crate) image: RgbaImage,
}

#[derive(Default, Debug)]
pub(crate) struct AssetRuntime {
    resources: Vec<SpriteResource>,
    by_key: HashMap<String, SpriteTextureId>,
    visuals: HashMap<String, ResolvedVisual>,
    next_id: u32,
}

impl AssetRuntime {
    pub(crate) fn new() -> Self {
        Self {
            resources: Vec::new(),
            by_key: HashMap::new(),
            visuals: HashMap::new(),
            // ART-R1 reserved the first renderer identity for Headwear.
            next_id: 1,
        }
    }

    /// Registers an embedded PNG once. Re-registering a key returns its
    /// existing identity without decoding or allocating another resource.
    pub(crate) fn register_png(
        &mut self,
        key: &str,
        bytes: &[u8],
    ) -> Result<SpriteTextureId, String> {
        if let Some(&id) = self.by_key.get(key) {
            return Ok(id);
        }
        let image = image::load_from_memory(bytes)
            .map_err(|err| format!("decode sprite resource {key}: {err}"))?
            .to_rgba8();
        let id = SpriteTextureId::from_raw(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        self.by_key.insert(key.to_owned(), id);
        self.resources.push(SpriteResource { id, image });
        Ok(id)
    }

    pub(crate) fn register_image(
        &mut self,
        key: &str,
        image: RgbaImage,
    ) -> Result<SpriteTextureId, String> {
        if let Some(&id) = self.by_key.get(key) {
            return Ok(id);
        }
        let id = SpriteTextureId::from_raw(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        self.by_key.insert(key.to_owned(), id);
        self.resources.push(SpriteResource { id, image });
        Ok(id)
    }

    pub(crate) fn register_visual(
        &mut self,
        key: &str,
        visual: ResolvedVisual,
    ) -> Result<(), String> {
        if self.visuals.insert(key.to_owned(), visual).is_some() {
            return Err(format!("duplicate visual key {key}"));
        }
        Ok(())
    }

    pub(crate) fn visual(&self, key: &str) -> Option<&ResolvedVisual> {
        self.visuals.get(key)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn resource(&self, id: SpriteTextureId) -> Option<&SpriteResource> {
        self.resources.iter().find(|resource| resource.id == id)
    }

    pub(crate) fn resources(&self) -> &[SpriteResource] {
        &self.resources
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn resource_count(&self) -> usize {
        self.resources.len()
    }
}
