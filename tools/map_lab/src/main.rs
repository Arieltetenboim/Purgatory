//! FORGE W1.3B Map Lab: faithful canonical-map preview and professional visual workflow.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use eframe::egui;
use egui::{Color32, Pos2, Rect, Sense, Stroke, TextureHandle, Vec2};
use purgatory_content::{
    CloudFieldAuthoring, CloudStackPosition, FootholdKind, FootholdPath, GameplaySpawnPoint,
    LoadMode, MIN_MAP_HEIGHT_WU, MIN_MAP_WIDTH_WU, ParallaxDepth, ParallaxFillMode, ParallaxLayer,
    Placement, PlacementKind, PortalLink, PresentationSprite, SkyGradient, TileTransform,
    cloud_field_seed, cloud_instance_count, cloud_instance_specs, default_content_root,
    load_registry, portal_runtime_authored, resolve_png_asset_folder,
};
use purgatory_map_lab::{
    MapImportCandidate, MapLabDocument, MapSwitchChoice, MapSwitchKind, PURGATORY_STANDARD_PPU,
    apply_map_switch_choice, classify_map_switch, discover_authored_maps,
    discover_unimported_tmx, import_numeric_tmx,
};

const CAMERA_HEIGHT_WU: f32 = purgatory_simulation::FOOTNOTE_TEST_VIEWPORT_HEIGHT;
const CAMERA_WIDTH_WU: f32 = CAMERA_HEIGHT_WU * purgatory_simulation::AOI_VIEWPORT_ASPECT;
const PLAYER_PRESENTATION_SCALE: f32 = 1.15;
const MAP_LAB_CLOUD_PREVIEW_SEED: u64 = 0x504D_4150_434C_4F55;
const PLAYER_REFERENCE_SIZE_WU: [f32; 2] = [
    purgatory_simulation::PLAYER_HALF_EXTENTS[0] * 2.0 * PLAYER_PRESENTATION_SCALE,
    purgatory_simulation::PLAYER_HALF_EXTENTS[1] * 2.0 * PLAYER_PRESENTATION_SCALE,
];

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1500.0, 900.0])
            .with_min_inner_size([1100.0, 700.0])
            .with_resizable(true)
            .with_title("PURGATORY Map Lab"),
        ..Default::default()
    };
    eframe::run_native(
        "PURGATORY Map Lab",
        options,
        Box::new(|cc| Ok(Box::new(MapLabApp::new(&cc.egui_ctx)))),
    )
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum EditorMode {
    #[default]
    Map,
    Footnote,
    Spawn,
    Entity,
    Environment,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum EntityCatalogKind {
    Portal,
    Npc,
    Interactable,
    Entity,
    Mob,
}

impl EntityCatalogKind {
    const ALL: [Self; 4] = [Self::Npc, Self::Interactable, Self::Entity, Self::Mob];

    const fn label(self) -> &'static str {
        match self {
            Self::Portal => "PORTALS",
            Self::Npc => "NPCS",
            Self::Interactable => "INTERACTABLES",
            Self::Entity => "ENTITIES",
            Self::Mob => "MOBS",
        }
    }

    const fn id_prefix(self) -> &'static str {
        match self {
            Self::Portal => "portal",
            Self::Npc => "npc",
            Self::Interactable => "interactable",
            Self::Entity => "entity",
            Self::Mob => "mob",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PortalTarget {
    map_authored: String,
    portal_id: String,
}

impl PortalTarget {
    fn label(&self) -> String {
        format!("{} · {}", self.map_authored, self.portal_id)
    }
}

#[derive(Clone, Debug)]
struct MonsterPreviewData {
    health_max: f32,
    movement_speed: f32,
    collision: [f32; 4],
    home_leash_radius: f32,
    sprite_id: Option<String>,
}

#[derive(Clone, Debug)]
struct EntityCatalogEntry {
    category: EntityCatalogKind,
    placement_kind: PlacementKind,
    authored_id: String,
    debug_name: String,
    monster_preview: Option<MonsterPreviewData>,
}

struct MapLabApp {
    path_text: String,
    document: Option<MapLabDocument>,
    textures: TextureRegions,
    environment_textures: HashMap<String, EnvironmentPreviewTexture>,
    cloud_textures: HashMap<String, Vec<EnvironmentPreviewTexture>>,
    preview_layers: Vec<bool>,
    ppu_text: String,
    compiled_ppu: Option<f32>,
    status: String,
    zoom: f32,
    pan: Vec2,
    fit_requested: bool,
    editor_mode: EditorMode,
    footnote_kind: FootholdKind,
    draft_points: Vec<[f32; 2]>,
    gameplay_dirty: bool,
    environment_dirty: bool,
    placements_dirty: bool,
    entity_catalog: Vec<EntityCatalogEntry>,
    portal_targets: Vec<PortalTarget>,
    portal_brush: bool,
    selected_catalog: Option<usize>,
    selected_placement: Option<usize>,
    selected_point: Option<(usize, usize)>,
    settings_open: bool,
    gradient_color_clipboard: Option<[u8; 4]>,
    available_maps: Vec<PathBuf>,
    unimported_maps: Vec<MapImportCandidate>,
    pending_map_switch: Option<PathBuf>,
}

impl MapLabApp {
    fn new(ctx: &egui::Context) -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../content/authoring/maps/map.map1.purgatory-map.json");
        let mut app = Self {
            path_text: path.display().to_string(),
            document: None,
            textures: HashMap::new(),
            environment_textures: HashMap::new(),
            cloud_textures: HashMap::new(),
            preview_layers: Vec::new(),
            ppu_text: String::new(),
            compiled_ppu: None,
            status: "Not compiled".to_owned(),
            zoom: 1.0,
            pan: Vec2::ZERO,
            fit_requested: true,
            editor_mode: EditorMode::Map,
            footnote_kind: FootholdKind::OneWay,
            draft_points: Vec::new(),
            gameplay_dirty: false,
            environment_dirty: false,
            placements_dirty: false,
            entity_catalog: Vec::new(),
            portal_targets: Vec::new(),
            portal_brush: false,
            selected_catalog: None,
            selected_placement: None,
            selected_point: None,
            settings_open: false,
            gradient_color_clipboard: None,
            available_maps: Vec::new(),
            unimported_maps: Vec::new(),
            pending_map_switch: None,
        };
        app.open(ctx, &path);
        app
    }

    fn open(&mut self, ctx: &egui::Context, path: &Path) {
        let document = match MapLabDocument::open(path) {
            Ok(document) => document,
            Err(error) => {
                self.status = format!("COMPILE ERROR\n{error}");
                return;
            }
        };
        if let Err(error) = self.install_document(ctx, document) {
            self.status = format!("ASSET ERROR\n{error}");
            return;
        }
        if let Some(document) = &self.document {
            self.ppu_text = document.source.pixels_per_world_unit.to_string();
        }
        self.clear_map_local_ui();
        self.gameplay_dirty = false;
        self.environment_dirty = false;
        self.placements_dirty = false;
        self.pending_map_switch = None;
        self.refresh_available_maps();
    }

    fn document_dirty(&self) -> bool {
        self.gameplay_dirty || self.environment_dirty || self.placements_dirty
    }

    fn refresh_available_maps(&mut self) {
        let directory = self
            .document
            .as_ref()
            .and_then(MapLabDocument::discover_directory)
            .map(Path::to_path_buf);
        if let Some(directory) = directory {
            self.available_maps = discover_authored_maps(&directory).unwrap_or_default();
            self.unimported_maps = discover_unimported_tmx(&directory).unwrap_or_default();
        } else {
            self.available_maps.clear();
            self.unimported_maps.clear();
        }
    }

    fn import_map(&mut self, ctx: &egui::Context, candidate: MapImportCandidate) {
        let Some(directory) = self
            .document
            .as_ref()
            .and_then(MapLabDocument::discover_directory)
            .map(Path::to_path_buf)
        else {
            self.status = "IMPORT ERROR\nNo authoring directory".to_owned();
            return;
        };
        match import_numeric_tmx(&directory, &candidate) {
            Ok(sidecar) => {
                self.refresh_available_maps();
                self.request_map_switch(ctx, sidecar, false);
            }
            Err(error) => self.status = format!("IMPORT ERROR\n{error}"),
        }
    }

    fn request_map_switch(&mut self, ctx: &egui::Context, target: PathBuf, reload: bool) {
        if target.as_os_str().is_empty() {
            self.status = "OPEN ERROR\nChoose a map sidecar".to_owned();
            return;
        }
        let same_document = self
            .document
            .as_ref()
            .is_some_and(|document| document.sidecar_path == target);
        match classify_map_switch(same_document, reload, self.document_dirty()) {
            MapSwitchKind::AlreadyOpen => {}
            MapSwitchKind::Open => self.open(ctx, &target),
            MapSwitchKind::Confirm => self.pending_map_switch = Some(target),
        }
    }

    fn confirm_pending_switch(&mut self, ctx: &egui::Context, choice: MapSwitchChoice) {
        let Some(target) = self.pending_map_switch.clone() else {
            return;
        };
        match apply_map_switch_choice(choice, || self.save_unsaved_documents()) {
            Ok(true) => self.open(ctx, &target),
            Ok(false) => self.pending_map_switch = None,
            Err(error) => self.status = format!("SAVE ERROR\n{error}"),
        }
    }

    fn save_unsaved_documents(&mut self) -> Result<(), String> {
        let Some(document) = &self.document else {
            return Err("No current map".to_owned());
        };
        if self.gameplay_dirty {
            document.save_gameplay()?;
            self.gameplay_dirty = false;
        }
        if self.environment_dirty {
            document.save_environment()?;
            self.environment_dirty = false;
        }
        if self.placements_dirty {
            document.save_placements()?;
            self.placements_dirty = false;
        }
        Ok(())
    }

    fn clear_map_local_ui(&mut self) {
        self.portal_brush = false;
        self.selected_catalog = None;
        self.selected_placement = None;
        self.selected_point = None;
        self.draft_points.clear();
    }

    fn apply_ppu(&mut self, ctx: &egui::Context) {
        let Ok(ppu) = self.ppu_text.trim().parse::<f32>() else {
            self.clear_compiled("COMPILE ERROR\nPPU must be a number");
            return;
        };
        let Some(mut document) = self.document.take() else {
            self.status = "COMPILE ERROR\nOpen a valid sidecar first".to_owned();
            return;
        };
        if let Err(error) = document.recompile(ppu) {
            self.clear_compiled(&format!("COMPILE ERROR\n{error}"));
            return;
        }
        if let Err(error) = self.install_document(ctx, document) {
            self.clear_compiled(&format!("ASSET ERROR\n{error}"));
        }
    }

    fn install_document(
        &mut self,
        ctx: &egui::Context,
        document: MapLabDocument,
    ) -> Result<(), String> {
        match (
            load_textures(ctx, &document),
            load_environment_textures(ctx, &document),
            load_entity_catalog(),
        ) {
            (
                Ok(textures),
                Ok((environment_textures, cloud_textures)),
                Ok((entity_catalog, portal_targets)),
            ) => {
                self.path_text = document.sidecar_path.display().to_string();
                self.preview_layers = document
                    .presentation
                    .layers
                    .iter()
                    .map(|layer| layer.visible)
                    .collect();
                self.compiled_ppu = Some(document.presentation.pixels_per_world_unit);
                self.status = format!(
                    "GREEN · schema v{} · {} layers · {} sprites · {} parallax · {} cloud fields · {} cloud PNGs",
                    document.presentation.schema_version,
                    document.presentation.layers.len(),
                    document
                        .presentation
                        .layers
                        .iter()
                        .map(|layer| layer.sprites.len())
                        .sum::<usize>(),
                    document.environment.parallax_layers.len(),
                    document.environment.cloud_fields.len(),
                    cloud_textures.values().map(Vec::len).sum::<usize>(),
                );
                self.document = Some(document);
                self.textures = textures;
                self.environment_textures = environment_textures;
                self.cloud_textures = cloud_textures;
                self.entity_catalog = entity_catalog;
                self.portal_targets = portal_targets;
                self.portal_brush = false;
                self.selected_catalog = None;
                self.selected_placement = None;
                self.selected_point = None;
                self.draft_points.clear();
                self.fit_requested = true;
                Ok(())
            }
            (Err(error), _, _) | (_, Err(error), _) | (_, _, Err(error)) => Err(error),
        }
    }

    fn clear_compiled(&mut self, status: &str) {
        self.document = None;
        self.textures.clear();
        self.environment_textures.clear();
        self.cloud_textures.clear();
        self.entity_catalog.clear();
        self.portal_targets.clear();
        self.portal_brush = false;
        self.selected_catalog = None;
        self.selected_placement = None;
        self.selected_point = None;
        self.draft_points.clear();
        self.preview_layers.clear();
        self.compiled_ppu = None;
        self.status = status.to_owned();
    }

    fn open_in_tiled(&mut self) {
        let Some(document) = &self.document else {
            self.status = "OPEN ERROR\nNo current compiled map".to_owned();
            return;
        };
        let Some(parent) = document.sidecar_path.parent() else {
            self.status = "OPEN ERROR\nInvalid sidecar path".to_owned();
            return;
        };
        let tmx = parent.join(&document.source.visual_source);
        if !tmx.is_file() {
            self.status = format!("OPEN ERROR\nTMX not found: {}", tmx.display());
            return;
        }

        #[cfg(windows)]
        {
            self.status = match std::process::Command::new("rundll32.exe")
                .arg("url.dll,FileProtocolHandler")
                .arg(&tmx)
                .spawn()
            {
                Ok(_) => format!("OPENED IN TILED\n{}", tmx.display()),
                Err(error) => format!("OPEN ERROR\n{}: {error}", tmx.display()),
            };
        }

        #[cfg(not(windows))]
        {
            self.status = "OPEN ERROR\nOpen in Tiled currently supports Windows only".to_owned();
        }
    }

    fn export_artifact(&mut self) {
        let Some(document) = &self.document else {
            self.status = "EXPORT ERROR\nNo current compiled output".to_owned();
            return;
        };
        let output = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/map-lab")
            .join(format!(
                "{}.map-presentation-v1.json",
                document.presentation.map_authored
            ));
        let result = (|| {
            let parent = output.parent().ok_or("invalid output path")?;
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("create {}: {error}", parent.display()))?;
            let bytes = document.canonical_json()?;
            std::fs::write(&output, bytes)
                .map_err(|error| format!("write {}: {error}", output.display()))
        })();
        self.status = match result {
            Ok(()) => format!("EXPORTED\n{}", output.display()),
            Err(error) => format!("EXPORT ERROR\n{error}"),
        };
    }

    fn enter_spawn_editor(&mut self) {
        self.editor_mode = EditorMode::Spawn;
        self.selected_point = None;
        self.draft_points.clear();
        self.status = "SPAWN EDIT · click near a FOOTNOTE to place default spawn".to_owned();
    }

    fn back_to_map(&mut self) {
        self.editor_mode = EditorMode::Map;
        self.draft_points.clear();
        self.selected_point = None;
        self.status = "MAP VIEW".to_owned();
    }

    fn enter_footnote_editor(&mut self) {
        self.editor_mode = EditorMode::Footnote;
        self.draft_points.clear();
        self.selected_point = None;
        self.status = "FOOTNOTE EDIT · click points to draw a path".to_owned();
    }

    fn back_from_footnote_editor(&mut self) {
        let had_draft = !self.draft_points.is_empty();
        self.back_to_map();
        if had_draft {
            self.status = "MAP VIEW · unfinished foothold path discarded".to_owned();
        }
    }

    fn finish_foothold_path(&mut self) {
        if self.draft_points.len() < 2 {
            self.status = "FOOTNOTE EDIT · add at least two points".to_owned();
            return;
        }
        let Some(document) = self.document.as_mut() else {
            return;
        };
        let next_index = document.gameplay.foothold_paths.len() + 1;
        document.gameplay.foothold_paths.push(FootholdPath {
            id: format!("foothold.{next_index:03}"),
            kind: self.footnote_kind,
            drop_through: self.footnote_kind == FootholdKind::OneWay,
            points: std::mem::take(&mut self.draft_points),
        });
        self.gameplay_dirty = true;
        self.status = format!(
            "FOOTNOTE EDIT · path foothold.{next_index:03} added · save gameplay to persist"
        );
    }

    fn cancel_foothold_path(&mut self) {
        self.draft_points.clear();
        self.status = "FOOTNOTE EDIT · current path cancelled".to_owned();
    }

    fn delete_foothold_path(&mut self, index: usize) {
        let Some(document) = self.document.as_mut() else {
            return;
        };
        if index < document.gameplay.foothold_paths.len() {
            let removed = document.gameplay.foothold_paths.remove(index);
            self.gameplay_dirty = true;
            self.status = format!("FOOTNOTE EDIT · deleted {}", removed.id);
        }
    }

    fn enter_entity_editor(&mut self) {
        self.editor_mode = EditorMode::Entity;
        self.selected_point = None;
        self.draft_points.clear();
        self.portal_brush = false;
        self.selected_catalog = None;
        self.selected_placement = None;
        self.status =
            "ENTITY · add a Portal or choose a library entry, then click the map".to_owned();
    }

    fn place_entity(&mut self, catalog_index: usize, position: [f32; 2]) {
        let Some(entry) = self.entity_catalog.get(catalog_index).cloned() else {
            return;
        };
        let Some(document) = self.document.as_mut() else {
            return;
        };
        let [min_x, min_y, max_x, max_y] = document.presentation.world_bounds;
        let id = next_placement_id(entry.category, &document.placements);
        document.placements.push(Placement {
            id: id.clone(),
            kind: entry.placement_kind,
            content_authored: entry.authored_id.clone(),
            position: [
                position[0].clamp(min_x, max_x),
                position[1].clamp(min_y, max_y),
            ],
            portal_link: None,
        });
        self.selected_placement = Some(document.placements.len() - 1);
        self.placements_dirty = true;
        self.status = format!("ENTITY · placed {id} · {}", entry.debug_name);
    }

    fn add_portal_brush(&mut self) {
        self.portal_brush = true;
        self.selected_catalog = None;
        self.selected_placement = None;
        self.status = "ENTITY · PORTAL brush · click the map to place a new portal".to_owned();
    }

    fn place_portal(&mut self, position: [f32; 2]) {
        let Some(document) = self.document.as_mut() else {
            return;
        };
        let [min_x, min_y, max_x, max_y] = document.presentation.world_bounds;
        let id = next_portal_id(&document.placements);
        let runtime_authored = portal_runtime_authored(&document.source.id, &id);
        document.placements.push(Placement {
            id: id.clone(),
            kind: PlacementKind::Portal,
            content_authored: runtime_authored,
            position: [
                position[0].clamp(min_x, max_x),
                position[1].clamp(min_y, max_y),
            ],
            portal_link: None,
        });
        self.selected_placement = Some(document.placements.len() - 1);
        self.portal_brush = false;
        self.placements_dirty = true;
        self.status = format!("ENTITY · placed {id} · choose Linked Portal");
    }

    fn set_selected_portal_link(&mut self, link: Option<PortalLink>) {
        let Some(index) = self.selected_placement else {
            return;
        };
        let Some(document) = self.document.as_mut() else {
            return;
        };
        let Some(placement) = document.placements.get_mut(index) else {
            return;
        };
        if placement.kind != PlacementKind::Portal {
            return;
        }
        placement.portal_link = link;
        self.placements_dirty = true;
    }

    fn portal_choices(&self) -> Vec<PortalTarget> {
        let mut targets = self.portal_targets.clone();
        if let Some(document) = &self.document {
            targets.retain(|target| target.map_authored != document.source.id);
            for placement in &document.placements {
                if placement.kind == PlacementKind::Portal {
                    targets.push(PortalTarget {
                        map_authored: document.source.id.clone(),
                        portal_id: placement.id.clone(),
                    });
                }
            }
        }
        targets.sort_by(|a, b| {
            a.map_authored
                .cmp(&b.map_authored)
                .then(a.portal_id.cmp(&b.portal_id))
        });
        targets.dedup();
        targets
    }

    fn edit_selected_placement(&mut self, x: f32, y: f32) {
        let Some(index) = self.selected_placement else {
            return;
        };
        let Some(document) = self.document.as_mut() else {
            return;
        };
        let [min_x, min_y, max_x, max_y] = document.presentation.world_bounds;
        let Some(placement) = document.placements.get_mut(index) else {
            self.selected_placement = None;
            return;
        };
        placement.position = [x.clamp(min_x, max_x), y.clamp(min_y, max_y)];
        self.placements_dirty = true;
    }

    fn delete_selected_placement(&mut self) {
        let Some(index) = self.selected_placement.take() else {
            return;
        };
        let Some(document) = self.document.as_mut() else {
            return;
        };
        if index < document.placements.len() {
            let removed = document.placements.remove(index);
            self.placements_dirty = true;
            self.status = format!("ENTITY · deleted {}", removed.id);
        }
    }

    fn save_placements(&mut self) {
        let Some(document) = &self.document else {
            self.status = "SAVE ERROR\nNo current map".to_owned();
            return;
        };
        self.status = match document.save_placements() {
            Ok(()) => {
                self.placements_dirty = false;
                format!(
                    "SAVED PLACEMENTS\n{}\nRestart the server to apply runtime content.",
                    document.placements_path.display()
                )
            }
            Err(error) => format!("SAVE ERROR\n{error}"),
        };
    }

    fn enter_environment_editor(&mut self) {
        self.editor_mode = EditorMode::Environment;
        self.selected_point = None;
        self.draft_points.clear();
        self.status = "ENVIRONMENT · edit map-level presentation".to_owned();
    }

    fn add_parallax_layer(&mut self) {
        let Some(document) = self.document.as_mut() else {
            return;
        };
        let next = document.environment.parallax_layers.len() + 1;
        let depth = ParallaxDepth::Far;
        document.environment.parallax_layers.push(ParallaxLayer {
            id: format!("background.{next:03}"),
            asset_path: "assets/maps/BG.png".to_owned(),
            depth,
            fill_mode: ParallaxFillMode::Repeat,
            parallax: depth.default_parallax(),
            offset_world: [0.0, 0.0],
            motion_world_per_second: [0.0, 0.0],
            repeat_x: true,
            repeat_y: false,
            opacity: 1.0,
        });
        self.environment_dirty = true;
        self.status = "ENVIRONMENT · parallax layer added · reload assets to preview".to_owned();
    }

    fn delete_parallax_layer(&mut self, index: usize) {
        let Some(document) = self.document.as_mut() else {
            return;
        };
        if index < document.environment.parallax_layers.len() {
            let removed = document.environment.parallax_layers.remove(index);
            self.environment_textures.remove(&removed.id);
            self.environment_dirty = true;
            self.status = format!("ENVIRONMENT · deleted {}", removed.id);
        }
    }

    fn add_foreground_layer(&mut self) {
        let Some(document) = self.document.as_mut() else {
            return;
        };
        let next = document.environment.foreground_layers.len() + 1;
        let depth = ParallaxDepth::Near;
        document.environment.foreground_layers.push(ParallaxLayer {
            id: format!("atmosphere.{next:03}"),
            asset_path: "assets/skys/foreground.png".to_owned(),
            depth,
            fill_mode: ParallaxFillMode::Cover,
            parallax: depth.default_parallax(),
            offset_world: [0.0, 0.0],
            motion_world_per_second: [0.0, 0.0],
            repeat_x: false,
            repeat_y: false,
            opacity: 0.5,
        });
        self.environment_dirty = true;
        self.status =
            "ENVIRONMENT · foreground atmosphere added · set Asset then Reload Assets".to_owned();
    }

    fn delete_foreground_layer(&mut self, index: usize) {
        let Some(document) = self.document.as_mut() else {
            return;
        };
        if index < document.environment.foreground_layers.len() {
            let removed = document.environment.foreground_layers.remove(index);
            self.environment_textures.remove(&removed.id);
            self.environment_dirty = true;
            self.status = format!("ENVIRONMENT · deleted {}", removed.id);
        }
    }

    fn add_cloud_field(&mut self) {
        let Some(document) = self.document.as_mut() else {
            return;
        };
        let next = document.environment.cloud_fields.len() + 1;
        let depth = ParallaxDepth::Far;
        document.environment.cloud_fields.push(CloudFieldAuthoring {
            id: format!("clouds.{next:03}"),
            asset_folder: "assets/skys/clouds".to_owned(),
            depth,
            stack_position: CloudStackPosition::for_depth(depth),
            parallax: depth.default_parallax(),
            density: 0.5,
            scale_range: depth.default_cloud_scale_range(),
            speed_range: depth.default_cloud_speed_range(),
            height_range: [0.58, 0.92],
            opacity_range: [0.65, 0.95],
        });
        self.environment_dirty = true;
        self.status = "ENVIRONMENT · cloud field added · set Folder then Reload Assets".to_owned();
    }

    fn delete_cloud_field(&mut self, index: usize) {
        let Some(document) = self.document.as_mut() else {
            return;
        };
        if index < document.environment.cloud_fields.len() {
            let removed = document.environment.cloud_fields.remove(index);
            self.cloud_textures.remove(&removed.id);
            self.environment_dirty = true;
            self.status = format!("ENVIRONMENT · deleted {}", removed.id);
        }
    }

    fn reload_environment_assets(&mut self, ctx: &egui::Context) {
        let Some(document) = &self.document else {
            return;
        };
        match load_environment_textures(ctx, document) {
            Ok((textures, cloud_textures)) => {
                self.environment_textures = textures;
                self.cloud_textures = cloud_textures;
                self.status = "ENVIRONMENT · preview assets reloaded".to_owned();
            }
            Err(error) => {
                self.status = format!("ASSET ERROR\n{error}");
            }
        }
    }

    fn save_environment(&mut self) {
        let Some(document) = &self.document else {
            self.status = "SAVE ERROR\nNo current map".to_owned();
            return;
        };
        self.status = match document.save_environment() {
            Ok(()) => {
                self.environment_dirty = false;
                format!(
                    "SAVED ENVIRONMENT\n{}\nRebuild the client to apply runtime presentation.",
                    document.environment_path.display()
                )
            }
            Err(error) => format!("SAVE ERROR\n{error}"),
        };
    }

    fn save_gameplay(&mut self) {
        let Some(document) = &self.document else {
            self.status = "SAVE ERROR\nNo current map".to_owned();
            return;
        };
        self.status = match document.save_gameplay() {
            Ok(()) => {
                self.gameplay_dirty = false;
                format!(
                    "SAVED GAMEPLAY\n{}\nRestart the server to apply runtime content.",
                    document.gameplay_path.display()
                )
            }
            Err(error) => format!("SAVE ERROR\n{error}"),
        };
    }

    fn select_foothold_point(&mut self, path_index: usize, point_index: usize) {
        self.selected_point = Some((path_index, point_index));
        self.status = format!(
            "FOOTNOTE EDIT · selected point {}:{}",
            path_index + 1,
            point_index + 1
        );
    }

    fn edit_selected_point(&mut self, x: f32, y: f32) {
        let Some((path_index, point_index)) = self.selected_point else {
            return;
        };
        let Some(document) = self.document.as_mut() else {
            return;
        };
        let [min_x, min_y, max_x, max_y] = document.presentation.world_bounds;
        let Some(path) = document.gameplay.foothold_paths.get_mut(path_index) else {
            return;
        };
        let Some(point) = path.points.get_mut(point_index) else {
            return;
        };
        *point = [x.clamp(min_x, max_x), y.clamp(min_y, max_y)];
        self.gameplay_dirty = true;
    }

    fn set_default_spawn(&mut self, approximate_center: [f32; 2]) {
        let Some(document) = self.document.as_ref() else {
            return;
        };
        let Some(snapped) = document.snap_spawn_to_foothold(approximate_center) else {
            self.status =
                "SPAWN EDIT · no FOOTNOTE surface close enough to clicked position".to_owned();
            return;
        };
        let Some(document) = self.document.as_mut() else {
            return;
        };
        if let Some(spawn) = document
            .gameplay
            .spawn_points
            .iter_mut()
            .find(|spawn| spawn.id == "default")
        {
            spawn.position = snapped;
        } else {
            document.gameplay.spawn_points.push(GameplaySpawnPoint {
                id: "default".to_owned(),
                position: snapped,
            });
        }
        self.gameplay_dirty = true;
        self.status = format!(
            "SPAWN EDIT · default = ({:.2}, {:.2})",
            snapped[0], snapped[1]
        );
    }

    fn edit_default_spawn(&mut self, x: f32, y: f32) {
        let Some(document) = self.document.as_ref() else {
            return;
        };
        let [min_x, min_y, max_x, max_y] = document.presentation.world_bounds;
        let approximate = [x.clamp(min_x, max_x), y.clamp(min_y, max_y)];
        let Some(snapped) = document.snap_spawn_to_foothold(approximate) else {
            self.status = "SPAWN EDIT · edited position has no nearby FOOTNOTE".to_owned();
            return;
        };
        let Some(document) = self.document.as_mut() else {
            return;
        };
        let Some(spawn) = document
            .gameplay
            .spawn_points
            .iter_mut()
            .find(|spawn| spawn.id == "default")
        else {
            return;
        };
        spawn.position = snapped;
        self.gameplay_dirty = true;
    }

    fn map_selector(&mut self, ui: &mut egui::Ui) {
        let current = self
            .document
            .as_ref()
            .map(|document| document.source.id.clone())
            .unwrap_or_else(|| "No map".to_owned());
        let current_path = self
            .document
            .as_ref()
            .map(|document| document.sidecar_path.clone());
        let maps = self.available_maps.clone();
        let imports = self.unimported_maps.clone();
        egui::ComboBox::from_id_salt("map_lab_map_selector")
            .selected_text(current)
            .width(190.0)
            .show_ui(ui, |ui| {
                if maps.is_empty() {
                    ui.label("No authored maps");
                }
                for path in maps {
                    let label = path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("map")
                        .trim_end_matches(".purgatory-map.json");
                    let selected = current_path.as_ref() == Some(&path);
                    if ui.selectable_label(selected, label).clicked() {
                        self.request_map_switch(ui.ctx(), path, false);
                    }
                }
                if !imports.is_empty() {
                    ui.separator();
                    ui.label("IMPORT NUMERIC TMX");
                    for candidate in imports {
                        let filename = candidate
                            .tmx_path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("map.tmx");
                        if ui
                            .button(format!(
                                "{filename} → {} · #{}",
                                candidate.authored_id, candidate.content_id
                            ))
                            .clicked()
                        {
                            self.import_map(ui.ctx(), candidate);
                        }
                    }
                }
            });
    }

    fn unsaved_switch_dialog(&mut self, ctx: &egui::Context) {
        let Some(target) = self.pending_map_switch.clone() else {
            return;
        };
        let label = target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("map")
            .trim_end_matches(".purgatory-map.json")
            .to_owned();
        let modal = egui::Modal::new(egui::Id::new("map_lab_unsaved_switch")).show(ctx, |ui| {
            ui.label(format!("Save changes before switching to {label}?"));
            ui.horizontal(|ui| {
                if ui.button("Save and switch").clicked() {
                    self.confirm_pending_switch(ctx, MapSwitchChoice::SaveAndSwitch);
                }
                if ui.button("Discard and switch").clicked() {
                    self.confirm_pending_switch(ctx, MapSwitchChoice::DiscardAndSwitch);
                }
                if ui.button("Cancel").clicked() {
                    self.confirm_pending_switch(ctx, MapSwitchChoice::Cancel);
                }
            });
        });
        if modal.should_close() && self.pending_map_switch.is_some() {
            self.confirm_pending_switch(ctx, MapSwitchChoice::Cancel);
        }
    }

    fn dirty(&self) -> bool {
        let ppu_dirty = self.compiled_ppu.is_some_and(|compiled| {
            self.ppu_text
                .trim()
                .parse::<f32>()
                .is_ok_and(|edited| (edited - compiled).abs() > f32::EPSILON)
        });
        ppu_dirty || self.gameplay_dirty || self.environment_dirty || self.placements_dirty
    }
}

impl eframe::App for MapLabApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if ui.input(|input| input.key_pressed(egui::Key::R) && input.modifiers.shift) {
            self.request_map_switch(ui.ctx(), PathBuf::from(self.path_text.trim()), true);
        }
        match self.editor_mode {
            EditorMode::Footnote => {
                if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                    self.back_from_footnote_editor();
                } else if ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                    self.finish_foothold_path();
                }
            }
            EditorMode::Spawn | EditorMode::Entity | EditorMode::Environment => {
                if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                    self.back_to_map();
                }
            }
            EditorMode::Map => {}
        }

        egui::Panel::top("map_lab_top")
            .exact_size(54.0)
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    match self.editor_mode {
                        EditorMode::Footnote => {
                            if ui.button("← BACK").clicked() {
                                self.back_from_footnote_editor();
                            }
                            ui.heading("FOOTNOTE");
                        }
                        EditorMode::Spawn => {
                            if ui.button("← BACK").clicked() {
                                self.back_to_map();
                            }
                            ui.heading("SPAWN");
                        }
                        EditorMode::Entity => {
                            if ui.button("← BACK").clicked() {
                                self.back_to_map();
                            }
                            ui.heading("ENTITY");
                        }
                        EditorMode::Environment => {
                            if ui.button("← BACK").clicked() {
                                self.back_to_map();
                            }
                            ui.heading("ENVIRONMENT");
                        }
                        EditorMode::Map => {
                            ui.heading("MAP LAB");
                            if ui.button("FOOTNOTE").clicked() && self.document.is_some() {
                                self.enter_footnote_editor();
                            }
                            if ui.button("SPAWN").clicked() && self.document.is_some() {
                                self.enter_spawn_editor();
                            }
                            if ui.button("ENTITY").clicked() && self.document.is_some() {
                                self.enter_entity_editor();
                            }
                            if ui.button("ENVIRONMENT").clicked() && self.document.is_some() {
                                self.enter_environment_editor();
                            }
                        }
                    }
                    ui.separator();
                    self.map_selector(ui);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.path_text)
                            .desired_width(280.0)
                            .hint_text("PURGATORY map sidecar"),
                    );
                    if ui.button("Open / Reload").clicked() {
                        self.request_map_switch(
                            ui.ctx(),
                            PathBuf::from(self.path_text.trim()),
                            true,
                        );
                    }
                    if ui.button("Open in Tiled").clicked() {
                        self.open_in_tiled();
                    }
                    if ui.button("Fit Map").clicked() {
                        self.fit_requested = true;
                    }
                    if ui.button("SETTINGS").clicked() {
                        self.settings_open = true;
                    }
                    if matches!(self.editor_mode, EditorMode::Footnote | EditorMode::Spawn) {
                        if ui.button("Save Gameplay").clicked() {
                            self.save_gameplay();
                        }
                    } else if self.editor_mode == EditorMode::Entity {
                        if ui.button("Save Placements").clicked() {
                            self.save_placements();
                        }
                    } else if self.editor_mode == EditorMode::Environment {
                        if ui.button("Save Environment").clicked() {
                            self.save_environment();
                        }
                    } else if ui.button("Export Canonical JSON").clicked() {
                        self.export_artifact();
                    }
                    ui.separator();
                    ui.colored_label(
                        if self.document.is_some() {
                            Color32::LIGHT_GREEN
                        } else {
                            Color32::LIGHT_RED
                        },
                        if self.dirty() {
                            "DIRTY · unsaved"
                        } else {
                            self.status.lines().next().unwrap_or("Not compiled")
                        },
                    );
                });
            });

        let mut delete_foothold = None;
        let mut delete_parallax = None;
        let mut delete_cloud = None;
        let mut delete_foreground = None;
        egui::Panel::left("map_lab_layers")
            .resizable(true)
            .default_size(match self.editor_mode {
                EditorMode::Map => 210.0,
                EditorMode::Entity => 300.0,
                _ => 250.0,
            })
            .min_size(180.0)
            .show(ui, |ui| {
                if self.editor_mode == EditorMode::Environment {
                    egui::ScrollArea::vertical()
                        .id_salt("map_lab_environment_scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.heading("ENVIRONMENT");
                            ui.small(
                                "Map-level presentation · independent from gameplay/Tiled visuals",
                            );
                            ui.separator();

                            egui::CollapsingHeader::new("SKY GRADIENT")
                                .default_open(true)
                                .show(ui, |ui| {
                                    let mut enabled = self
                                        .document
                                        .as_ref()
                                        .and_then(|document| document.environment.sky_gradient)
                                        .is_some();
                                    if ui.checkbox(&mut enabled, "Enabled").changed()
                                        && let Some(document) = self.document.as_mut()
                                    {
                                        document.environment.sky_gradient =
                                            enabled.then(SkyGradient::default);
                                        self.environment_dirty = true;
                                    }
                                    if enabled {
                                        let gradient = self
                                            .document
                                            .as_ref()
                                            .and_then(|document| {
                                                document.environment.sky_gradient
                                            })
                                            .unwrap_or_default();
                                        let mut top = Color32::from_rgba_unmultiplied(
                                            gradient.top_rgba[0],
                                            gradient.top_rgba[1],
                                            gradient.top_rgba[2],
                                            gradient.top_rgba[3],
                                        );
                                        let mut bottom = Color32::from_rgba_unmultiplied(
                                            gradient.bottom_rgba[0],
                                            gradient.bottom_rgba[1],
                                            gradient.bottom_rgba[2],
                                            gradient.bottom_rgba[3],
                                        );
                                        ui.horizontal(|ui| {
                                            ui.label("Top");
                                            if ui.color_edit_button_srgba(&mut top).changed()
                                                && let Some(document) = self.document.as_mut()
                                                && let Some(sky) =
                                                    document.environment.sky_gradient.as_mut()
                                            {
                                                sky.top_rgba = top.to_array();
                                                self.environment_dirty = true;
                                            }
                                            if ui.small_button("Copy").clicked() {
                                                self.gradient_color_clipboard =
                                                    Some(top.to_array());
                                            }
                                            let clipboard = self.gradient_color_clipboard;
                                            if ui
                                                .add_enabled(
                                                    clipboard.is_some(),
                                                    egui::Button::new("Paste"),
                                                )
                                                .clicked()
                                                && let Some(rgba) = clipboard
                                                && let Some(document) = self.document.as_mut()
                                                && let Some(sky) =
                                                    document.environment.sky_gradient.as_mut()
                                            {
                                                top = Color32::from_rgba_unmultiplied(
                                                    rgba[0], rgba[1], rgba[2], rgba[3],
                                                );
                                                sky.top_rgba = rgba;
                                                self.environment_dirty = true;
                                            }
                                        });
                                        ui.horizontal(|ui| {
                                            ui.label("Bottom");
                                            if ui.color_edit_button_srgba(&mut bottom).changed()
                                                && let Some(document) = self.document.as_mut()
                                                && let Some(sky) =
                                                    document.environment.sky_gradient.as_mut()
                                            {
                                                sky.bottom_rgba = bottom.to_array();
                                                self.environment_dirty = true;
                                            }
                                            if ui.small_button("Copy").clicked() {
                                                self.gradient_color_clipboard =
                                                    Some(bottom.to_array());
                                            }
                                            let clipboard = self.gradient_color_clipboard;
                                            if ui
                                                .add_enabled(
                                                    clipboard.is_some(),
                                                    egui::Button::new("Paste"),
                                                )
                                                .clicked()
                                                && let Some(rgba) = clipboard
                                                && let Some(document) = self.document.as_mut()
                                                && let Some(sky) =
                                                    document.environment.sky_gradient.as_mut()
                                            {
                                                bottom = Color32::from_rgba_unmultiplied(
                                                    rgba[0], rgba[1], rgba[2], rgba[3],
                                                );
                                                sky.bottom_rgba = rgba;
                                                self.environment_dirty = true;
                                            }
                                        });
                                        ui.small(
                                            "Preview updates immediately. Runtime applies after client rebuild.",
                                        );
                                    }
                                });

                            ui.separator();
                            egui::CollapsingHeader::new("PARALLAX BACKGROUNDS")
                                .default_open(true)
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        if ui.button("+ Add Background").clicked() {
                                            self.add_parallax_layer();
                                        }
                                        if ui.button("Reload Assets").clicked() {
                                            self.reload_environment_assets(ui.ctx());
                                        }
                                    });
                                    ui.small(
                                        "Asset paths are relative to Graphic/. Natural image size uses the map PPU.",
                                    );

                                    if let Some(document) = self.document.as_mut() {
                                        for (index, layer) in document
                                            .environment
                                            .parallax_layers
                                            .iter_mut()
                                            .enumerate()
                                        {
                                            ui.separator();
                                            egui::CollapsingHeader::new(&layer.id)
                                                .id_salt(("parallax_layer", index))
                                                .default_open(index == 0)
                                                .show(ui, |ui| {
                                                    ui.horizontal(|ui| {
                                                        ui.label("ID");
                                                        if ui
                                                            .text_edit_singleline(&mut layer.id)
                                                            .changed()
                                                        {
                                                            self.environment_dirty = true;
                                                        }
                                                        if ui.small_button("Delete").clicked() {
                                                            delete_parallax = Some(index);
                                                        }
                                                    });
                                                    ui.label("Asset");
                                                    if ui
                                                        .text_edit_singleline(&mut layer.asset_path)
                                                        .changed()
                                                    {
                                                        self.environment_dirty = true;
                                                    }

                                                    let old_depth = layer.depth;
                                                    egui::ComboBox::from_id_salt((
                                                        "parallax_depth",
                                                        index,
                                                    ))
                                                    .selected_text(layer.depth.as_str())
                                                    .show_ui(ui, |ui| {
                                                        for depth in ParallaxDepth::ALL {
                                                            ui.selectable_value(
                                                                &mut layer.depth,
                                                                depth,
                                                                depth.as_str(),
                                                            );
                                                        }
                                                    });
                                                    if layer.depth != old_depth {
                                                        layer.parallax =
                                                            layer.depth.default_parallax();
                                                        self.environment_dirty = true;
                                                    }

                                                    let old_fill = layer.fill_mode;
                                                    egui::ComboBox::from_id_salt((
                                                        "parallax_fill",
                                                        index,
                                                    ))
                                                    .selected_text(layer.fill_mode.as_str())
                                                    .show_ui(ui, |ui| {
                                                        for mode in ParallaxFillMode::ALL {
                                                            ui.selectable_value(
                                                                &mut layer.fill_mode,
                                                                mode,
                                                                mode.as_str(),
                                                            );
                                                        }
                                                    });
                                                    if layer.fill_mode != old_fill {
                                                        self.environment_dirty = true;
                                                    }
                                                    ui.small(match layer.fill_mode {
                                                        ParallaxFillMode::Natural => {
                                                            "Natural size · single copy"
                                                        }
                                                        ParallaxFillMode::Repeat => {
                                                            "Natural size · tile on enabled axes"
                                                        }
                                                        ParallaxFillMode::Stretch => {
                                                            "Stretch to guaranteed parallax coverage"
                                                        }
                                                        ParallaxFillMode::Fit => {
                                                            "Preserve aspect · fit inside coverage"
                                                        }
                                                        ParallaxFillMode::Cover => {
                                                            "Preserve aspect · fully cover camera travel"
                                                        }
                                                    });

                                                    if ui
                                                        .add(
                                                            egui::Slider::new(
                                                                &mut layer.parallax,
                                                                0.0..=1.0,
                                                            )
                                                            .text("Parallax"),
                                                        )
                                                        .changed()
                                                    {
                                                        self.environment_dirty = true;
                                                    }
                                                    ui.small("0 = screen-fixed · 1 = world-locked");
                                                    if ui
                                                        .add(
                                                            egui::Slider::new(
                                                                &mut layer.opacity,
                                                                0.0..=1.0,
                                                            )
                                                            .text("Opacity"),
                                                        )
                                                        .changed()
                                                    {
                                                        self.environment_dirty = true;
                                                    }
                                                    if layer.fill_mode
                                                        == ParallaxFillMode::Repeat
                                                    {
                                                        ui.horizontal(|ui| {
                                                            if ui
                                                                .checkbox(
                                                                    &mut layer.repeat_x,
                                                                    "Repeat X",
                                                                )
                                                                .changed()
                                                            {
                                                                self.environment_dirty = true;
                                                            }
                                                            if ui
                                                                .checkbox(
                                                                    &mut layer.repeat_y,
                                                                    "Repeat Y",
                                                                )
                                                                .changed()
                                                            {
                                                                self.environment_dirty = true;
                                                            }
                                                        });
                                                    }
                                                    ui.horizontal(|ui| {
                                                        ui.label("Offset");
                                                        if ui
                                                            .add(
                                                                egui::DragValue::new(
                                                                    &mut layer.offset_world[0],
                                                                )
                                                                .speed(0.05)
                                                                .prefix("X "),
                                                            )
                                                            .changed()
                                                        {
                                                            self.environment_dirty = true;
                                                        }
                                                        if ui
                                                            .add(
                                                                egui::DragValue::new(
                                                                    &mut layer.offset_world[1],
                                                                )
                                                                .speed(0.05)
                                                                .prefix("Y "),
                                                            )
                                                            .changed()
                                                        {
                                                            self.environment_dirty = true;
                                                        }
                                                    });
                                                    ui.horizontal(|ui| {
                                                        ui.label("Motion");
                                                        if ui
                                                            .add(
                                                                egui::DragValue::new(
                                                                    &mut layer
                                                                        .motion_world_per_second[0],
                                                                )
                                                                .speed(0.01)
                                                                .prefix("X "),
                                                            )
                                                            .changed()
                                                        {
                                                            self.environment_dirty = true;
                                                        }
                                                        if ui
                                                            .add(
                                                                egui::DragValue::new(
                                                                    &mut layer
                                                                        .motion_world_per_second[1],
                                                                )
                                                                .speed(0.01)
                                                                .prefix("Y "),
                                                            )
                                                            .changed()
                                                        {
                                                            self.environment_dirty = true;
                                                        }
                                                    });
                                                    ui.small(
                                                        "Motion uses world units/second · Repeat motion wraps seamlessly.",
                                                    );
                                                });
                                        }
                                    }
                                });

                            ui.separator();
                            egui::CollapsingHeader::new("CLOUD FIELDS")
                                .default_open(true)
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        if ui.button("+ Add Cloud Field").clicked() {
                                            self.add_cloud_field();
                                        }
                                        if ui.button("Reload Assets").clicked() {
                                            self.reload_environment_assets(ui.ctx());
                                        }
                                    });
                                    ui.small(
                                        "Folder-backed · client-local seeded presentation · minimum 1 PNG.",
                                    );

                                    if let Some(document) = self.document.as_mut() {
                                        for (index, field) in document
                                            .environment
                                            .cloud_fields
                                            .iter_mut()
                                            .enumerate()
                                        {
                                            ui.separator();
                                            egui::CollapsingHeader::new(&field.id)
                                                .id_salt(("cloud_field", index))
                                                .default_open(index == 0)
                                                .show(ui, |ui| {
                                                    ui.horizontal(|ui| {
                                                        ui.label("ID");
                                                        if ui
                                                            .text_edit_singleline(&mut field.id)
                                                            .changed()
                                                        {
                                                            self.environment_dirty = true;
                                                        }
                                                        if ui.small_button("Delete").clicked() {
                                                            delete_cloud = Some(index);
                                                        }
                                                    });
                                                    ui.label("Folder");
                                                    if ui
                                                        .text_edit_singleline(
                                                            &mut field.asset_folder,
                                                        )
                                                        .changed()
                                                    {
                                                        self.environment_dirty = true;
                                                    }
                                                    let found = self
                                                        .cloud_textures
                                                        .get(&field.id)
                                                        .map_or(0, Vec::len);
                                                    ui.colored_label(
                                                        if found > 0 {
                                                            Color32::LIGHT_GREEN
                                                        } else {
                                                            Color32::YELLOW
                                                        },
                                                        format!(
                                                            "Found: {found} PNG{}{}",
                                                            if found == 1 { "" } else { "s" },
                                                            if found == 0 {
                                                                " · Reload Assets after setting folder"
                                                            } else {
                                                                ""
                                                            }
                                                        ),
                                                    );

                                                    ui.label("Depth behavior");
                                                    let old_depth = field.depth;
                                                    egui::ComboBox::from_id_salt((
                                                        "cloud_depth",
                                                        index,
                                                    ))
                                                    .selected_text(field.depth.as_str())
                                                    .show_ui(ui, |ui| {
                                                        for depth in ParallaxDepth::ALL {
                                                            ui.selectable_value(
                                                                &mut field.depth,
                                                                depth,
                                                                depth.as_str(),
                                                            );
                                                        }
                                                    });
                                                    if field.depth != old_depth {
                                                        field.parallax =
                                                            field.depth.default_parallax();
                                                        field.stack_position =
                                                            CloudStackPosition::for_depth(
                                                                field.depth,
                                                            );
                                                        self.environment_dirty = true;
                                                    }

                                                    ui.label("Stack Position");
                                                    let old_stack = field.stack_position;
                                                    egui::ComboBox::from_id_salt((
                                                        "cloud_stack",
                                                        index,
                                                    ))
                                                    .selected_text(
                                                        field.stack_position.as_str(),
                                                    )
                                                    .show_ui(ui, |ui| {
                                                        for position in CloudStackPosition::ALL {
                                                            ui.selectable_value(
                                                                &mut field.stack_position,
                                                                position,
                                                                position.as_str(),
                                                            );
                                                        }
                                                    });
                                                    if field.stack_position != old_stack {
                                                        self.environment_dirty = true;
                                                    }
                                                    ui.small(
                                                        "Draw order only. Example: After Far = before Mid.",
                                                    );

                                                    if ui
                                                        .add(
                                                            egui::Slider::new(
                                                                &mut field.parallax,
                                                                0.0..=1.0,
                                                            )
                                                            .text("Parallax"),
                                                        )
                                                        .changed()
                                                    {
                                                        self.environment_dirty = true;
                                                    }
                                                    if ui
                                                        .add(
                                                            egui::Slider::new(
                                                                &mut field.density,
                                                                0.0..=1.0,
                                                            )
                                                            .text("Density"),
                                                        )
                                                        .changed()
                                                    {
                                                        self.environment_dirty = true;
                                                    }
                                                    if range_row(
                                                        ui,
                                                        "Scale",
                                                        &mut field.scale_range,
                                                        0.01,
                                                    ) {
                                                        self.environment_dirty = true;
                                                    }
                                                    if range_row(
                                                        ui,
                                                        "Speed",
                                                        &mut field.speed_range,
                                                        0.01,
                                                    ) {
                                                        self.environment_dirty = true;
                                                    }
                                                    if range_row(
                                                        ui,
                                                        "Height",
                                                        &mut field.height_range,
                                                        0.01,
                                                    ) {
                                                        self.environment_dirty = true;
                                                    }
                                                    ui.small(
                                                        "Height is normalized: 0 = bottom · 1 = top.",
                                                    );
                                                    if range_row(
                                                        ui,
                                                        "Opacity",
                                                        &mut field.opacity_range,
                                                        0.01,
                                                    ) {
                                                        self.environment_dirty = true;
                                                    }
                                                });
                                        }
                                    }
                                });

                            ui.separator();
                            egui::CollapsingHeader::new("FOREGROUND ATMOSPHERE")
                                .default_open(true)
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        if ui.button("+ Add Atmosphere").clicked() {
                                            self.add_foreground_layer();
                                        }
                                        if ui.button("Reload Assets").clicked() {
                                            self.reload_environment_assets(ui.ctx());
                                        }
                                    });
                                    ui.small(
                                        "Drawn over world art, players, monsters, NPCs and items · below debug/UI.",
                                    );
                                    if let Some(document) = self.document.as_mut() {
                                        for (index, layer) in document
                                            .environment
                                            .foreground_layers
                                            .iter_mut()
                                            .enumerate()
                                        {
                                            ui.separator();
                                            egui::CollapsingHeader::new(&layer.id)
                                                .id_salt(("foreground_layer", index))
                                                .default_open(index == 0)
                                                .show(ui, |ui| {
                                                    ui.horizontal(|ui| {
                                                        ui.label("ID");
                                                        if ui
                                                            .text_edit_singleline(&mut layer.id)
                                                            .changed()
                                                        {
                                                            self.environment_dirty = true;
                                                        }
                                                        if ui.small_button("Delete").clicked() {
                                                            delete_foreground = Some(index);
                                                        }
                                                    });
                                                    if environment_layer_controls(
                                                        ui,
                                                        layer,
                                                        ("foreground_controls", index),
                                                    ) {
                                                        self.environment_dirty = true;
                                                    }
                                                });
                                        }
                                    }
                                });

                            ui.separator();
                            ui.label("NEXT");
                            ui.small("Ambient sprite fields · weather events");
                            ui.add_space(12.0);
                        });
                } else if self.editor_mode == EditorMode::Entity {
                    egui::ScrollArea::vertical()
                        .id_salt("map_lab_entity_scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.heading("ENTITY");
                            ui.small("Server placement authoring · definitions stay owned by their source labs");
                            ui.separator();

                            ui.horizontal(|ui| {
                                if ui
                                    .selectable_label(self.portal_brush, "+ Add Portal")
                                    .clicked()
                                {
                                    self.add_portal_brush();
                                }
                                if (self.portal_brush || self.selected_catalog.is_some())
                                    && ui.small_button("Clear Brush").clicked()
                                {
                                    self.portal_brush = false;
                                    self.selected_catalog = None;
                                    self.status =
                                        "ENTITY · placement brush cleared · click a marker to select it"
                                            .to_owned();
                                }
                            });
                            ui.small("Portal is map-owned. Place it first, then choose its Linked Portal.");
                            ui.separator();
                            ui.label("LIBRARY");
                            for category in EntityCatalogKind::ALL {
                                egui::CollapsingHeader::new(category.label())
                                    .default_open(matches!(
                                        category,
                                        EntityCatalogKind::Npc | EntityCatalogKind::Mob
                                    ))
                                    .show(ui, |ui| {
                                        for (index, entry) in self
                                            .entity_catalog
                                            .iter()
                                            .enumerate()
                                            .filter(|(_, entry)| entry.category == category)
                                        {
                                            let selected = self.selected_catalog == Some(index);
                                            if ui
                                                .selectable_label(
                                                    selected,
                                                    format!(
                                                        "{}\n  {}",
                                                        entry.debug_name, entry.authored_id
                                                    ),
                                                )
                                                .clicked()
                                            {
                                                self.portal_brush = false;
                                                self.selected_catalog = Some(index);
                                                self.selected_placement = None;
                                                self.status = format!(
                                                    "ENTITY · {} selected · click map to place",
                                                    entry.authored_id
                                                );
                                            }
                                        }
                                    });
                            }

                            ui.separator();
                            ui.heading("PLACEMENTS");
                            let selected_data = self.selected_placement.and_then(|index| {
                                self.document.as_ref().and_then(|document| {
                                    document
                                        .placements
                                        .get(index)
                                        .map(|placement| (index, placement.clone()))
                                })
                            });
                            if let Some((index, placement)) = selected_data {
                                let selected_entry =
                                    catalog_entry_for_placement(&placement, &self.entity_catalog)
                                        .cloned();
                                let map_id = self
                                    .document
                                    .as_ref()
                                    .map(|document| document.source.id.clone())
                                    .unwrap_or_default();
                                let portal_choices = self.portal_choices();
                                ui.group(|ui| {
                                    if placement.kind == PlacementKind::Portal {
                                        ui.heading("PORTAL");
                                        ui.label(format!("Map ID: {map_id}"));
                                        ui.label(format!("Portal ID: {}", placement.id));
                                        ui.separator();
                                        ui.label("Linked Portal");
                                        let selected_text = placement
                                            .portal_link
                                            .as_ref()
                                            .map(|link| {
                                                format!(
                                                    "{} · {}",
                                                    link.map_authored, link.portal_id
                                                )
                                            })
                                            .unwrap_or_else(|| "Unlinked".to_owned());
                                        let mut chosen = placement.portal_link.clone();
                                        egui::ComboBox::from_id_salt((
                                            "linked_portal",
                                            &placement.id,
                                        ))
                                        .selected_text(selected_text)
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(&mut chosen, None, "Unlinked");
                                            ui.separator();
                                            for target in &portal_choices {
                                                if target.map_authored == map_id
                                                    && target.portal_id == placement.id
                                                {
                                                    continue;
                                                }
                                                ui.selectable_value(
                                                    &mut chosen,
                                                    Some(PortalLink {
                                                        map_authored: target.map_authored.clone(),
                                                        portal_id: target.portal_id.clone(),
                                                    }),
                                                    target.label(),
                                                );
                                            }
                                        });
                                        if chosen != placement.portal_link {
                                            self.set_selected_portal_link(chosen);
                                        }
                                    } else if let Some(entry) = &selected_entry {
                                        ui.heading(entry.category.label());
                                        ui.label(format!("Placement ID: {}", placement.id));
                                        ui.label(format!("Content: {}", placement.content_authored));
                                        ui.small(format!("Name: {}", entry.debug_name));
                                        if let Some(monster) = &entry.monster_preview {
                                            ui.separator();
                                            ui.label("Mob Definition · read-only");
                                            ui.label(format!("HP: {:.1}", monster.health_max));
                                            ui.label(format!(
                                                "Move Speed: {:.2}",
                                                monster.movement_speed
                                            ));
                                            ui.label(format!(
                                                "Collision L/R/B/T: {:.2} / {:.2} / {:.2} / {:.2}",
                                                monster.collision[0],
                                                monster.collision[1],
                                                monster.collision[2],
                                                monster.collision[3]
                                            ));
                                            ui.label(format!(
                                                "Home Leash: {:.2} wu",
                                                monster.home_leash_radius
                                            ));
                                            if let Some(sprite_id) = &monster.sprite_id {
                                                ui.small(format!("Sprite: {sprite_id}"));
                                            }
                                            ui.small(
                                                "Edit gameplay/presentation in Mob Lab; Map Lab owns placement only.",
                                            );
                                        }
                                    } else {
                                        ui.label(format!("Selected: {}", placement.id));
                                        ui.small(format!(
                                            "{} · {}",
                                            placement.kind.as_str(),
                                            placement.content_authored
                                        ));
                                    }
                                    let mut x = placement.position[0];
                                    let mut y = placement.position[1];
                                    ui.horizontal(|ui| {
                                        ui.label("X");
                                        let x_changed = ui
                                            .add(egui::DragValue::new(&mut x).speed(0.05))
                                            .changed();
                                        ui.label("Y");
                                        let y_changed = ui
                                            .add(egui::DragValue::new(&mut y).speed(0.05))
                                            .changed();
                                        if x_changed || y_changed {
                                            self.edit_selected_placement(x, y);
                                        }
                                    });
                                    if ui.button("Delete Placement").clicked() {
                                        self.delete_selected_placement();
                                    }
                                    ui.small(format!("#{}", index + 1));
                                });
                            } else {
                                ui.small("Select a marker on the map to inspect or move it.");
                            }

                            if let Some(document) = &self.document {
                                ui.separator();
                                ui.label(format!("{} placement(s)", document.placements.len()));
                                for (index, placement) in document.placements.iter().enumerate() {
                                    let selected = self.selected_placement == Some(index);
                                    if ui
                                        .selectable_label(
                                            selected,
                                            if placement.kind == PlacementKind::Portal {
                                                format!("{} · PORTAL", placement.id)
                                            } else {
                                                format!(
                                                    "{} · {}",
                                                    placement.id, placement.content_authored
                                                )
                                            },
                                        )
                                        .clicked()
                                    {
                                        self.selected_placement = Some(index);
                                        self.selected_catalog = None;
                                    }
                                }
                            }
                            ui.separator();
                            if ui.button("Save Placements").clicked() {
                                self.save_placements();
                            }
                            ui.small(
                                "+ Add Portal or choose Library → click map · Portal Inspector shows Map ID + Portal ID + Linked Portal · Save writes Placement V2.",
                            );
                        });
                } else if self.editor_mode == EditorMode::Footnote {
                    ui.heading("FOOTNOTE EDIT");
                    ui.small("Polyline authoring · gameplay-owned · Tiled stays visual-only");
                    ui.separator();

                    ui.label("CATEGORY");
                    ui.horizontal(|ui| {
                        let one_way = self.footnote_kind == FootholdKind::OneWay;
                        if ui
                            .selectable_label(one_way, "● OneWay")
                            .on_hover_text("Land from above; pass through from below")
                            .clicked()
                        {
                            self.footnote_kind = FootholdKind::OneWay;
                        }
                        let solid = self.footnote_kind == FootholdKind::Solid;
                        if ui
                            .selectable_label(solid, "● Solid")
                            .on_hover_text("Blocks from both sides")
                            .clicked()
                        {
                            self.footnote_kind = FootholdKind::Solid;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.colored_label(foothold_color(FootholdKind::OneWay), "━━ OneWay");
                        ui.colored_label(foothold_color(FootholdKind::Solid), "━━ Solid");
                    });
                    ui.separator();

                    ui.label(format!(
                        "Current path: {} point(s)",
                        self.draft_points.len()
                    ));
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                self.draft_points.len() >= 2,
                                egui::Button::new("Finish Path"),
                            )
                            .clicked()
                        {
                            self.finish_foothold_path();
                        }
                        if ui
                            .add_enabled(!self.draft_points.is_empty(), egui::Button::new("Cancel"))
                            .clicked()
                        {
                            self.cancel_foothold_path();
                        }
                    });
                    ui.small(
                        "Click map to add points · Space+drag pans without ending the path · Enter finishes · BACK/Esc exits",
                    );
                    ui.separator();

                    ui.heading("PATHS");
                    if let Some((path_index, point_index)) = self.selected_point {
                        if let Some(document) = &self.document
                            && let Some(point) = document
                                .gameplay
                                .foothold_paths
                                .get(path_index)
                                .and_then(|path| path.points.get(point_index))
                                .copied()
                        {
                            let mut x = point[0];
                            let mut y = point[1];
                            ui.group(|ui| {
                                ui.label(format!(
                                    "Selected point {}:{}",
                                    path_index + 1,
                                    point_index + 1
                                ));
                                ui.horizontal(|ui| {
                                    ui.label("X");
                                    let x_changed = ui
                                        .add(egui::DragValue::new(&mut x).speed(0.05))
                                        .changed();
                                    ui.label("Y");
                                    let y_changed = ui
                                        .add(egui::DragValue::new(&mut y).speed(0.05))
                                        .changed();
                                    if x_changed || y_changed {
                                        self.edit_selected_point(x, y);
                                    }
                                });
                            });
                        }
                        ui.add_space(6.0);
                    }
                    if let Some(document) = &self.document {
                        if document.gameplay.foothold_paths.is_empty() {
                            ui.label("No authored footholds yet.");
                        }
                        for (index, path) in document.gameplay.foothold_paths.iter().enumerate() {
                            ui.horizontal(|ui| {
                                ui.colored_label(foothold_color(path.kind), "━━");
                                ui.label(format!(
                                    "{} · {:?} · {} pts",
                                    path.id,
                                    path.kind,
                                    path.points.len()
                                ));
                                if ui.small_button("×").clicked() {
                                    delete_foothold = Some(index);
                                }
                            });
                        }
                    }
                    ui.separator();
                    if ui.button("Save Gameplay").clicked() {
                        self.save_gameplay();
                    }
                } else if self.editor_mode == EditorMode::Spawn {
                    ui.heading("SPAWN EDIT");
                    ui.small("Player AABB center · snapped to authored FOOTNOTE support");
                    ui.separator();

                    let default_spawn = self.document.as_ref().and_then(|document| {
                        document
                            .gameplay
                            .spawn_points
                            .iter()
                            .find(|spawn| spawn.id == "default")
                            .map(|spawn| spawn.position)
                    });
                    if let Some(point) = default_spawn {
                        let mut x = point[0];
                        let mut y = point[1];
                        ui.label("DEFAULT SPAWN");
                        ui.horizontal(|ui| {
                            ui.label("X");
                            let x_changed = ui
                                .add(egui::DragValue::new(&mut x).speed(0.05))
                                .changed();
                            ui.label("Y");
                            let y_changed = ui
                                .add(egui::DragValue::new(&mut y).speed(0.05))
                                .changed();
                            if x_changed || y_changed {
                                self.edit_default_spawn(x, y);
                            }
                        });
                        ui.small("Y is the player AABB center; edits snap back to FOOTNOTE.");
                    } else {
                        ui.colored_label(Color32::YELLOW, "No default spawn authored.");
                    }
                    ui.separator();
                    ui.label("Click near a FOOTNOTE to place/move the default spawn.");
                    ui.label("Space+Drag pans · wheel zoom · BACK/Esc exits.");
                    ui.separator();
                    if ui.button("Save Gameplay").clicked() {
                        self.save_gameplay();
                    }
                } else {
                    ui.heading("LAYERS");
                    ui.label("Preview-only visibility");
                    ui.separator();
                    if let Some(document) = &self.document {
                        for (index, layer) in document.presentation.layers.iter().enumerate() {
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut self.preview_layers[index], "");
                                ui.label(&layer.name);
                            });
                            ui.small(format!(
                                "{:?} · {} sprite{}{}",
                                layer.kind,
                                layer.sprites.len(),
                                if layer.sprites.len() == 1 { "" } else { "s" },
                                if layer.visible {
                                    ""
                                } else {
                                    " · source hidden"
                                }
                            ));
                            ui.add_space(5.0);
                        }
                    } else {
                        ui.label("No current compiler output.");
                    }
                }
            });
        if let Some(index) = delete_foothold {
            self.delete_foothold_path(index);
        }
        if let Some(index) = delete_parallax {
            self.delete_parallax_layer(index);
        }
        if let Some(index) = delete_cloud {
            self.delete_cloud_field(index);
        }
        if let Some(index) = delete_foreground {
            self.delete_foreground_layer(index);
        }

        egui::Panel::right("map_lab_info")
            .resizable(true)
            .default_size(270.0)
            .min_size(230.0)
            .show(ui, |ui| {
                ui.heading("MAP INFO");
                ui.separator();
                let mut apply_ppu_requested = false;
                if let Some(document) = &self.document {
                    let map = &document.presentation;
                    ui.label(format!("Map: {}", map.map_authored));
                    ui.label(format!(
                        "TMX: {} × {} px",
                        map.visual_extent_px[0], map.visual_extent_px[1]
                    ));
                    ui.horizontal(|ui| {
                        ui.label("PPU");
                        ui.add(egui::TextEdit::singleline(&mut self.ppu_text).desired_width(90.0));
                        ui.label(format!("· standard {:.0}", PURGATORY_STANDARD_PPU));
                    });
                    ui.horizontal(|ui| {
                        apply_ppu_requested = ui.button("Apply Preview Override").clicked();
                        if ui.button("Use Standard 100").clicked() {
                            self.ppu_text = PURGATORY_STANDARD_PPU.to_string();
                            apply_ppu_requested = true;
                        }
                    });
                    let authored_ppu = document.source.pixels_per_world_unit;
                    if (authored_ppu - PURGATORY_STANDARD_PPU).abs() <= f32::EPSILON {
                        ui.colored_label(
                            Color32::LIGHT_GREEN,
                            "STANDARD SCALE · authored sidecar = 100 px/wu",
                        );
                    } else {
                        ui.colored_label(
                            Color32::YELLOW,
                            format!(
                                "NON-STANDARD AUTHORED SCALE · {:.3} px/wu",
                                authored_ppu
                            ),
                        );
                    }
                    ui.small(
                        "Preview overrides are temporary. Map1 production scale is 100 px/wu; camera zoom is separate.",
                    );
                    let width = map.world_bounds[2] - map.world_bounds[0];
                    let height = map.world_bounds[3] - map.world_bounds[1];
                    ui.label(format!("World: {width:.3} × {height:.3} wu"));
                    let width_coverage = width / CAMERA_WIDTH_WU * 100.0;
                    let height_coverage = height / CAMERA_HEIGHT_WU * 100.0;
                    if map_covers_camera([width, height]) {
                        ui.colored_label(
                            Color32::LIGHT_GREEN,
                            format!(
                                "MAP CONTRACT PASS · {:.0}% width · {:.0}% height",
                                width_coverage, height_coverage
                            ),
                        );
                        ui.small(format!(
                            "Hard minimum: {:.3} × {:.3} wu (one full gameplay camera).",
                            MIN_MAP_WIDTH_WU, MIN_MAP_HEIGHT_WU
                        ));
                    } else {
                        ui.colored_label(
                            Color32::LIGHT_RED,
                            format!(
                                "MAP CONTRACT FAIL · minimum {:.3} × {:.3} wu",
                                MIN_MAP_WIDTH_WU, MIN_MAP_HEIGHT_WU
                            ),
                        );
                    }
                    ui.separator();
                    ui.label(format!(
                        "Camera: {:.3} × {:.3} wu",
                        CAMERA_WIDTH_WU, CAMERA_HEIGHT_WU
                    ));
                    ui.label(format!(
                        "Player ref: {:.3} × {:.3} wu",
                        PLAYER_REFERENCE_SIZE_WU[0], PLAYER_REFERENCE_SIZE_WU[1]
                    ));
                    ui.small("Player reference = current 1.15× presentation-scaled gameplay body.");
                }
                if apply_ppu_requested {
                    self.apply_ppu(ui.ctx());
                }
                ui.separator();
                ui.heading("COMPILER / VALIDATION");
                if let Some(document) = &self.document {
                    match document.gameplay_readiness() {
                        Ok(()) => {
                            ui.colored_label(Color32::LIGHT_GREEN, "GAMEPLAY READY");
                        }
                        Err(reason) => {
                            ui.colored_label(Color32::YELLOW, format!("NOT READY · {reason}"));
                        }
                    }
                }
                ui.add(egui::Label::new(&self.status).wrap().selectable(true));
            });

        if self.settings_open {
            let mut open = self.settings_open;
            egui::Window::new("MAP LAB SETTINGS")
                .open(&mut open)
                .resizable(true)
                .default_width(420.0)
                .show(ui.ctx(), |ui| {
                    ui.heading("MAP");
                    if let Some(document) = self.document.as_mut() {
                        ui.horizontal(|ui| {
                            ui.label("Name");
                            if ui
                                .text_edit_singleline(&mut document.gameplay.name)
                                .changed()
                            {
                                self.gameplay_dirty = true;
                            }
                        });
                        ui.label(format!("Authored ID: {}", document.gameplay.map_authored));
                        ui.label(format!(
                            "World bounds: {:.2}..{:.2} × {:.2}..{:.2}",
                            document.presentation.world_bounds[0],
                            document.presentation.world_bounds[2],
                            document.presentation.world_bounds[1],
                            document.presentation.world_bounds[3]
                        ));
                        ui.small("Authored ID and bounds are source-owned and read-only here.");
                    }
                    ui.separator();
                    ui.heading("SHORTCUTS");
                    egui::Grid::new("map_lab_shortcuts").show(ui, |ui| {
                        ui.label("Shift+R");
                        ui.label("Reload map");
                        ui.end_row();
                        ui.label("Wheel");
                        ui.label("Zoom");
                        ui.end_row();
                        ui.label("Space+Drag");
                        ui.label("Pan while editing FOOTNOTE / SPAWN");
                        ui.end_row();
                        ui.label("Click");
                        ui.label("Add/select FOOTNOTE point or place SPAWN");
                        ui.end_row();
                        ui.label("Enter");
                        ui.label("Finish current FOOTNOTE path");
                        ui.end_row();
                        ui.label("Esc / BACK");
                        ui.label("Leave current editor");
                        ui.end_row();
                    });
                    ui.separator();
                    if ui.button("Save Gameplay").clicked() {
                        self.save_gameplay();
                    }
                });
            self.settings_open = open;
        }

        self.unsaved_switch_dialog(ui.ctx());

        egui::Panel::bottom("map_lab_status")
            .exact_size(28.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("Zoom {:.0}%", self.zoom * 100.0));
                    ui.separator();
                    ui.label(format!("Pan {:.0}, {:.0}px", self.pan.x, self.pan.y));
                    ui.separator();
                    ui.label(match self.editor_mode {
                        EditorMode::Footnote => {
                            "Click add point · Space+drag pan · wheel zoom · Enter finish · Esc/BACK exit"
                        }
                        EditorMode::Spawn => {
                            "Click place spawn · Space+drag pan · wheel zoom · Esc/BACK exit"
                        }
                        EditorMode::Entity => {
                            "Library select → click place · marker click selects · X/Y moves · Save Placements persists"
                        }
                        EditorMode::Environment => {
                            "Edit environment · preview is live · Save Environment persists"
                        }
                        EditorMode::Map => {
                            "Drag to pan · wheel to zoom · Shift+R reload · source files are not rewritten"
                        }
                    });
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            self.preview(ui);
        });
    }
}

impl MapLabApp {
    fn preview(&mut self, ui: &mut egui::Ui) {
        let (canvas, response) =
            ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        ui.painter()
            .rect_filled(canvas, 0.0, Color32::from_rgb(24, 27, 32));
        let pan_gesture = self.editor_mode == EditorMode::Map
            || ui.input(|input| input.key_down(egui::Key::Space));
        if pan_gesture && response.dragged() {
            self.pan += ui.input(|input| input.pointer.delta());
        }
        if response.hovered() {
            let scroll = ui.input(|input| input.smooth_scroll_delta.y);
            if scroll.abs() > f32::EPSILON {
                self.zoom = (self.zoom * (scroll * 0.0015).exp()).clamp(0.1, 12.0);
            }
        }
        if self.fit_requested {
            self.zoom = 1.0;
            self.pan = Vec2::ZERO;
            self.fit_requested = false;
        }

        let Some(document) = &self.document else {
            ui.painter().text(
                canvas.center(),
                egui::Align2::CENTER_CENTER,
                "No current canonical map\nSee compiler status",
                egui::FontId::proportional(18.0),
                Color32::LIGHT_RED,
            );
            return;
        };
        let map = &document.presentation;
        let world_width = map.world_bounds[2] - map.world_bounds[0];
        let world_height = map.world_bounds[3] - map.world_bounds[1];
        let base_scale =
            ((canvas.width() - 40.0) / world_width).min((canvas.height() - 40.0) / world_height);
        let scale = base_scale.max(0.01) * self.zoom;
        let center = canvas.center() + self.pan;
        let to_screen = |world: [f32; 2]| {
            Pos2::new(
                center.x + (world[0] - world_width * 0.5) * scale,
                center.y - (world[1] - world_height * 0.5) * scale,
            )
        };
        let map_rect = Rect::from_two_pos(
            to_screen([0.0, world_height]),
            to_screen([world_width, 0.0]),
        );
        let map_clip = map_rect.intersect(canvas);
        ui.painter()
            .rect_filled(map_rect, 0.0, Color32::from_rgb(10, 12, 15));
        let map_painter = ui.painter().with_clip_rect(map_clip);
        if let Some(gradient) = document.environment.sky_gradient {
            const BANDS: usize = 24;
            for index in 0..BANDS {
                let t0 = index as f32 / BANDS as f32;
                let t1 = (index + 1) as f32 / BANDS as f32;
                let color = lerp_color32(gradient.bottom_rgba, gradient.top_rgba, (t0 + t1) * 0.5);
                let y0 = min_y_screen(map_rect, t0);
                let y1 = min_y_screen(map_rect, t1);
                map_painter.rect_filled(
                    Rect::from_min_max(
                        Pos2::new(map_rect.left(), y1),
                        Pos2::new(map_rect.right(), y0),
                    ),
                    0.0,
                    color,
                );
            }
        }

        let preview_camera = [world_width * 0.5, world_height * 0.5];
        let environment_time_seconds = ui.input(|input| input.time);
        if document
            .environment
            .parallax_layers
            .iter()
            .chain(document.environment.foreground_layers.iter())
            .any(|layer| {
                layer
                    .motion_world_per_second
                    .iter()
                    .any(|value| value.abs() > f32::EPSILON)
            })
            || document.environment.cloud_fields.iter().any(|field| {
                field
                    .speed_range
                    .iter()
                    .any(|value| value.abs() > f32::EPSILON)
            })
        {
            ui.ctx().request_repaint();
        }
        for field in document
            .environment
            .cloud_fields
            .iter()
            .filter(|field| field.stack_position == CloudStackPosition::BeforeAll)
        {
            if let Some(textures) = self.cloud_textures.get(&field.id) {
                paint_cloud_field_preview(
                    &map_painter,
                    field,
                    textures,
                    map.pixels_per_world_unit,
                    [world_width * 0.5, world_height * 0.5],
                    [world_width, world_height],
                    preview_camera,
                    [CAMERA_WIDTH_WU, CAMERA_HEIGHT_WU],
                    environment_time_seconds,
                    &to_screen,
                );
            }
        }

        for depth in ParallaxDepth::ALL {
            for layer in document
                .environment
                .parallax_layers
                .iter()
                .filter(|layer| layer.depth == depth)
            {
                if let Some(texture) = self.environment_textures.get(&layer.id) {
                    paint_parallax_preview(
                        &map_painter,
                        layer,
                        texture,
                        map.pixels_per_world_unit,
                        [world_width * 0.5, world_height * 0.5],
                        [world_width, world_height],
                        preview_camera,
                        [CAMERA_WIDTH_WU, CAMERA_HEIGHT_WU],
                        environment_time_seconds,
                        &to_screen,
                    );
                }
            }

            let stack_position = CloudStackPosition::for_depth(depth);
            for field in document
                .environment
                .cloud_fields
                .iter()
                .filter(|field| field.stack_position == stack_position)
            {
                if let Some(textures) = self.cloud_textures.get(&field.id) {
                    paint_cloud_field_preview(
                        &map_painter,
                        field,
                        textures,
                        map.pixels_per_world_unit,
                        [world_width * 0.5, world_height * 0.5],
                        [world_width, world_height],
                        preview_camera,
                        [CAMERA_WIDTH_WU, CAMERA_HEIGHT_WU],
                        environment_time_seconds,
                        &to_screen,
                    );
                }
            }
        }

        for (index, layer) in map.layers.iter().enumerate() {
            if !self.preview_layers.get(index).copied().unwrap_or(false) {
                continue;
            }
            for sprite in &layer.sprites {
                if sprite.visible {
                    paint_sprite(
                        &map_painter,
                        sprite,
                        layer.opacity,
                        &self.textures,
                        &to_screen,
                    );
                }
            }
        }

        for layer in &document.environment.foreground_layers {
            if let Some(texture) = self.environment_textures.get(&layer.id) {
                paint_parallax_preview(
                    &map_painter,
                    layer,
                    texture,
                    map.pixels_per_world_unit,
                    [world_width * 0.5, world_height * 0.5],
                    [world_width, world_height],
                    preview_camera,
                    [CAMERA_WIDTH_WU, CAMERA_HEIGHT_WU],
                    environment_time_seconds,
                    &to_screen,
                );
            }
        }

        for (path_index, path) in document.gameplay.foothold_paths.iter().enumerate() {
            paint_foothold_path(&map_painter, path, &to_screen, false);
            if let Some((selected_path, selected_point)) = self.selected_point
                && selected_path == path_index
                && let Some(point) = path.points.get(selected_point)
            {
                map_painter.circle_stroke(to_screen(*point), 7.0, Stroke::new(2.0, Color32::WHITE));
            }
        }
        if self.editor_mode == EditorMode::Footnote && !self.draft_points.is_empty() {
            paint_draft_foothold(
                &map_painter,
                &self.draft_points,
                self.footnote_kind,
                &to_screen,
            );
        }

        for spawn in &document.gameplay.spawn_points {
            let center = to_screen(spawn.position);
            let half = purgatory_simulation::PLAYER_HALF_EXTENTS;
            let spawn_rect = Rect::from_two_pos(
                to_screen([spawn.position[0] - half[0], spawn.position[1] + half[1]]),
                to_screen([spawn.position[0] + half[0], spawn.position[1] - half[1]]),
            );
            map_painter.rect_stroke(
                spawn_rect,
                2.0,
                Stroke::new(2.0, Color32::from_rgb(120, 255, 120)),
                egui::StrokeKind::Inside,
            );
            map_painter.circle_filled(center, 5.0, Color32::from_rgb(120, 255, 120));
            map_painter.text(
                center + Vec2::new(7.0, -7.0),
                egui::Align2::LEFT_BOTTOM,
                &spawn.id,
                egui::FontId::monospace(11.0),
                Color32::from_rgb(160, 255, 160),
            );
        }

        for (index, placement) in document.placements.iter().enumerate() {
            let center = to_screen(placement.position);
            let selected = self.selected_placement == Some(index);
            let color = placement_color(placement, &self.entity_catalog);
            let entry = catalog_entry_for_placement(placement, &self.entity_catalog);

            if self.editor_mode == EditorMode::Entity {
                if let Some(monster) = entry.and_then(|entry| entry.monster_preview.as_ref()) {
                    let [left, right, bottom, top] = monster.collision;
                    let collision_rect = Rect::from_two_pos(
                        to_screen([placement.position[0] - left, placement.position[1]]),
                        to_screen([
                            placement.position[0] + right,
                            placement.position[1] + bottom + top,
                        ]),
                    );
                    map_painter.rect_filled(
                        collision_rect,
                        0.0,
                        Color32::from_rgba_unmultiplied(
                            255,
                            80,
                            80,
                            if selected { 44 } else { 22 },
                        ),
                    );
                    map_painter.rect_stroke(
                        collision_rect,
                        0.0,
                        Stroke::new(if selected { 2.0 } else { 1.0 }, color),
                        egui::StrokeKind::Inside,
                    );
                    let runtime_home =
                        to_screen([placement.position[0], placement.position[1] + bottom]);
                    map_painter.circle_stroke(
                        runtime_home,
                        monster.home_leash_radius * scale,
                        Stroke::new(
                            if selected { 1.5 } else { 0.75 },
                            Color32::from_rgba_unmultiplied(255, 120, 90, 150),
                        ),
                    );
                }

                if placement.kind == PlacementKind::Portal {
                    let radius = if selected { 10.0 } else { 8.0 };
                    let points = [
                        center + Vec2::new(0.0, -radius),
                        center + Vec2::new(radius, 0.0),
                        center + Vec2::new(0.0, radius),
                        center + Vec2::new(-radius, 0.0),
                    ];
                    for edge in 0..points.len() {
                        map_painter.line_segment(
                            [points[edge], points[(edge + 1) % points.len()]],
                            Stroke::new(if selected { 2.0 } else { 1.0 }, color),
                        );
                    }
                    if let Some(link) = &placement.portal_link {
                        if link.map_authored == document.source.id {
                            if let Some(target) = document.placements.iter().find(|candidate| {
                                candidate.kind == PlacementKind::Portal
                                    && candidate.id == link.portal_id
                            }) {
                                map_painter.line_segment(
                                    [center, to_screen(target.position)],
                                    Stroke::new(
                                        if selected { 2.0 } else { 1.0 },
                                        Color32::from_rgba_unmultiplied(80, 210, 255, 150),
                                    ),
                                );
                            }
                        } else if selected {
                            map_painter.text(
                                center + Vec2::new(8.0, 14.0),
                                egui::Align2::LEFT_TOP,
                                format!("→ {} · {}", link.map_authored, link.portal_id),
                                egui::FontId::monospace(10.0),
                                color,
                            );
                        }
                    }
                }
            }

            map_painter.circle_filled(center, if selected { 7.0 } else { 5.0 }, color);
            map_painter.circle_stroke(
                center,
                if selected { 9.0 } else { 7.0 },
                Stroke::new(if selected { 2.0 } else { 1.0 }, Color32::WHITE),
            );
            let kind_label = if placement.kind == PlacementKind::Portal {
                "PORTAL"
            } else {
                entry
                    .map(|entry| entry.category.label())
                    .unwrap_or("ENTITY")
            };
            map_painter.text(
                center + Vec2::new(8.0, -8.0),
                egui::Align2::LEFT_BOTTOM,
                format!("{kind_label} · {}", placement.id),
                egui::FontId::monospace(10.0),
                color,
            );
        }

        let clicked_existing_point = if self.editor_mode == EditorMode::Footnote
            && !ui.input(|input| input.key_down(egui::Key::Space))
            && response.clicked()
        {
            response.interact_pointer_pos().and_then(|position| {
                document
                    .gameplay
                    .foothold_paths
                    .iter()
                    .enumerate()
                    .flat_map(|(path_index, path)| {
                        path.points
                            .iter()
                            .enumerate()
                            .map(move |(point_index, point)| {
                                (
                                    path_index,
                                    point_index,
                                    to_screen(*point).distance(position),
                                )
                            })
                    })
                    .filter(|(_, _, distance)| *distance <= 8.0)
                    .min_by(|a, b| a.2.total_cmp(&b.2))
                    .map(|(path_index, point_index, _)| (path_index, point_index))
            })
        } else {
            None
        };

        let clicked_world = if self.editor_mode == EditorMode::Footnote
            && clicked_existing_point.is_none()
            && !ui.input(|input| input.key_down(egui::Key::Space))
            && response.clicked()
        {
            response.interact_pointer_pos().and_then(|position| {
                map_rect.contains(position).then(|| {
                    [
                        ((position.x - center.x) / scale + world_width * 0.5)
                            .clamp(0.0, world_width),
                        (world_height * 0.5 - (position.y - center.y) / scale)
                            .clamp(0.0, world_height),
                    ]
                })
            })
        } else {
            None
        };

        let clicked_spawn_world = if self.editor_mode == EditorMode::Spawn
            && !ui.input(|input| input.key_down(egui::Key::Space))
            && response.clicked()
        {
            response.interact_pointer_pos().and_then(|position| {
                map_rect.contains(position).then(|| {
                    [
                        ((position.x - center.x) / scale + world_width * 0.5)
                            .clamp(0.0, world_width),
                        (world_height * 0.5 - (position.y - center.y) / scale)
                            .clamp(0.0, world_height),
                    ]
                })
            })
        } else {
            None
        };

        let clicked_entity_placement = if self.editor_mode == EditorMode::Entity
            && !ui.input(|input| input.key_down(egui::Key::Space))
            && response.clicked()
        {
            response.interact_pointer_pos().and_then(|position| {
                document
                    .placements
                    .iter()
                    .enumerate()
                    .map(|(index, placement)| {
                        (index, to_screen(placement.position).distance(position))
                    })
                    .filter(|(_, distance)| *distance <= 10.0)
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(index, _)| index)
            })
        } else {
            None
        };

        let clicked_entity_world = if self.editor_mode == EditorMode::Entity
            && clicked_entity_placement.is_none()
            && !ui.input(|input| input.key_down(egui::Key::Space))
            && response.clicked()
        {
            response.interact_pointer_pos().and_then(|position| {
                map_rect.contains(position).then(|| {
                    [
                        ((position.x - center.x) / scale + world_width * 0.5)
                            .clamp(0.0, world_width),
                        (world_height * 0.5 - (position.y - center.y) / scale)
                            .clamp(0.0, world_height),
                    ]
                })
            })
        } else {
            None
        };

        let camera_center = [world_width * 0.5, world_height * 0.5];
        let camera_rect = world_rect(
            camera_center,
            [CAMERA_WIDTH_WU, CAMERA_HEIGHT_WU],
            &to_screen,
        );
        ui.painter().rect_filled(
            camera_rect.intersect(canvas),
            0.0,
            Color32::from_rgba_unmultiplied(60, 160, 255, 22),
        );
        ui.painter().rect_stroke(
            camera_rect,
            0.0,
            Stroke::new(2.0, Color32::from_rgb(80, 180, 255)),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            camera_rect.left_top() + Vec2::new(5.0, 5.0),
            egui::Align2::LEFT_TOP,
            format!("GAME CAMERA {:.3} × 13 wu", CAMERA_WIDTH_WU),
            egui::FontId::monospace(12.0),
            Color32::from_rgb(120, 205, 255),
        );

        let player_center = [world_width * 0.5, PLAYER_REFERENCE_SIZE_WU[1] * 0.5];
        let player_rect = world_rect(player_center, PLAYER_REFERENCE_SIZE_WU, &to_screen);
        ui.painter().rect_filled(
            player_rect,
            3.0,
            Color32::from_rgba_unmultiplied(255, 220, 100, 150),
        );
        ui.painter().rect_stroke(
            player_rect,
            3.0,
            Stroke::new(1.5, Color32::from_rgb(255, 230, 120)),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            player_rect.center_top() - Vec2::new(0.0, 4.0),
            egui::Align2::CENTER_BOTTOM,
            "PLAYER SCALE",
            egui::FontId::monospace(11.0),
            Color32::from_rgb(255, 230, 120),
        );
        ui.painter().rect_stroke(
            map_rect,
            0.0,
            Stroke::new(2.0, Color32::WHITE),
            egui::StrokeKind::Inside,
        );
        if let Some((path_index, point_index)) = clicked_existing_point {
            self.select_foothold_point(path_index, point_index);
        } else if let Some(point) = clicked_world {
            self.selected_point = None;
            self.draft_points.push(point);
            self.status = format!(
                "FOOTNOTE EDIT · point {} = ({:.2}, {:.2})",
                self.draft_points.len(),
                point[0],
                point[1]
            );
        } else if let Some(point) = clicked_spawn_world {
            self.set_default_spawn(point);
        } else if let Some(index) = clicked_entity_placement {
            self.selected_placement = Some(index);
            self.portal_brush = false;
            self.selected_catalog = None;
            if let Some(placement) = self
                .document
                .as_ref()
                .and_then(|document| document.placements.get(index))
            {
                self.status = format!("ENTITY · selected {}", placement.id);
            }
        } else if let Some(point) = clicked_entity_world {
            if self.portal_brush {
                self.place_portal(point);
            } else if let Some(catalog_index) = self.selected_catalog {
                self.place_entity(catalog_index, point);
            } else {
                self.status =
                    "ENTITY · use + Add Portal or choose a library entry before placing".to_owned();
            }
        }
    }
}

fn load_entity_catalog() -> Result<(Vec<EntityCatalogEntry>, Vec<PortalTarget>), String> {
    let registry = load_registry(&default_content_root(), LoadMode::Full)
        .map_err(|error| error.to_string())?;
    let mut entries = Vec::new();
    let mut portal_targets = Vec::new();
    for map in registry.iter_maps() {
        for placement in registry.placements(&map.authored_id) {
            match placement.kind {
                PlacementKind::Portal => portal_targets.push(PortalTarget {
                    map_authored: map.authored_id.clone(),
                    portal_id: placement.id.clone(),
                }),
                PlacementKind::Entity => {
                    if registry
                        .entity(&placement.content_authored)
                        .is_some_and(|entity| {
                            entity.interactable
                                == Some(purgatory_simulation::InteractableKind::Portal)
                        })
                    {
                        portal_targets.push(PortalTarget {
                            map_authored: map.authored_id.clone(),
                            portal_id: placement.content_authored.clone(),
                        });
                    }
                }
                PlacementKind::Monster => {}
            }
        }
    }
    portal_targets.sort_by(|a, b| {
        a.map_authored
            .cmp(&b.map_authored)
            .then(a.portal_id.cmp(&b.portal_id))
    });
    portal_targets.dedup();

    for entity in registry.iter_entities() {
        let category = match entity.interactable {
            Some(purgatory_simulation::InteractableKind::Portal) => EntityCatalogKind::Portal,
            Some(purgatory_simulation::InteractableKind::Npc) => EntityCatalogKind::Npc,
            Some(
                purgatory_simulation::InteractableKind::Generic
                | purgatory_simulation::InteractableKind::Chest
                | purgatory_simulation::InteractableKind::Switch,
            ) => EntityCatalogKind::Interactable,
            None => EntityCatalogKind::Entity,
        };
        entries.push(EntityCatalogEntry {
            category,
            placement_kind: PlacementKind::Entity,
            authored_id: entity.authored_id.clone(),
            debug_name: entity.debug_name.clone(),
            monster_preview: None,
        });
    }
    for monster in registry.iter_monsters() {
        entries.push(EntityCatalogEntry {
            category: EntityCatalogKind::Mob,
            placement_kind: PlacementKind::Monster,
            authored_id: monster.authored_id.clone(),
            debug_name: monster.debug_name.clone(),
            monster_preview: Some(MonsterPreviewData {
                health_max: monster.health_max,
                movement_speed: monster.movement_speed,
                collision: [
                    monster.collision_bounds.left,
                    monster.collision_bounds.right,
                    monster.collision_bounds.bottom,
                    monster.collision_bounds.top,
                ],
                home_leash_radius: monster.home_leash_radius,
                sprite_id: registry
                    .monster_presentation(&monster.authored_id)
                    .map(|presentation| presentation.sprite_id.clone()),
            }),
        });
    }
    entries.sort_by(|a, b| {
        a.category
            .cmp(&b.category)
            .then(a.authored_id.cmp(&b.authored_id))
    });
    Ok((entries, portal_targets))
}

fn next_portal_id(placements: &[Placement]) -> String {
    for index in 1..=9999 {
        let candidate = format!("portal.{index:03}");
        if placements.iter().all(|placement| placement.id != candidate) {
            return candidate;
        }
    }
    "portal.overflow".to_owned()
}

fn next_placement_id(category: EntityCatalogKind, placements: &[Placement]) -> String {
    let prefix = category.id_prefix();
    for index in 1..=9999 {
        let candidate = format!("placement.{prefix}_{index:03}");
        if placements.iter().all(|placement| placement.id != candidate) {
            return candidate;
        }
    }
    format!("placement.{prefix}_overflow")
}

fn catalog_entry_for_placement<'a>(
    placement: &Placement,
    catalog: &'a [EntityCatalogEntry],
) -> Option<&'a EntityCatalogEntry> {
    catalog.iter().find(|entry| {
        entry.placement_kind == placement.kind && entry.authored_id == placement.content_authored
    })
}

fn placement_color(placement: &Placement, catalog: &[EntityCatalogEntry]) -> Color32 {
    if placement.kind == PlacementKind::Portal {
        return Color32::from_rgb(80, 210, 255);
    }
    let category = catalog_entry_for_placement(placement, catalog).map(|entry| entry.category);
    match category {
        Some(EntityCatalogKind::Portal) => Color32::from_rgb(80, 210, 255),
        Some(EntityCatalogKind::Npc) => Color32::from_rgb(255, 220, 90),
        Some(EntityCatalogKind::Interactable) => Color32::from_rgb(190, 120, 255),
        Some(EntityCatalogKind::Entity) => Color32::from_rgb(180, 180, 190),
        Some(EntityCatalogKind::Mob) => Color32::from_rgb(255, 100, 90),
        None if placement.kind == PlacementKind::Monster => Color32::from_rgb(255, 100, 90),
        None => Color32::LIGHT_GRAY,
    }
}

fn environment_layer_controls(
    ui: &mut egui::Ui,
    layer: &mut ParallaxLayer,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
) -> bool {
    let mut changed = false;

    ui.label("Asset");
    changed |= ui.text_edit_singleline(&mut layer.asset_path).changed();

    let old_depth = layer.depth;
    egui::ComboBox::from_id_salt(("environment_layer_depth", id_salt))
        .selected_text(layer.depth.as_str())
        .show_ui(ui, |ui| {
            for depth in ParallaxDepth::ALL {
                ui.selectable_value(&mut layer.depth, depth, depth.as_str());
            }
        });
    if layer.depth != old_depth {
        layer.parallax = layer.depth.default_parallax();
        changed = true;
    }

    let old_fill = layer.fill_mode;
    egui::ComboBox::from_id_salt(("environment_layer_fill", &layer.id))
        .selected_text(layer.fill_mode.as_str())
        .show_ui(ui, |ui| {
            for mode in ParallaxFillMode::ALL {
                ui.selectable_value(&mut layer.fill_mode, mode, mode.as_str());
            }
        });
    changed |= layer.fill_mode != old_fill;

    changed |= ui
        .add(egui::Slider::new(&mut layer.parallax, 0.0..=1.0).text("Parallax"))
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut layer.opacity, 0.0..=1.0).text("Opacity"))
        .changed();

    if layer.fill_mode == ParallaxFillMode::Repeat {
        ui.horizontal(|ui| {
            changed |= ui.checkbox(&mut layer.repeat_x, "Repeat X").changed();
            changed |= ui.checkbox(&mut layer.repeat_y, "Repeat Y").changed();
        });
    }

    ui.horizontal(|ui| {
        ui.label("Offset");
        changed |= ui
            .add(
                egui::DragValue::new(&mut layer.offset_world[0])
                    .speed(0.05)
                    .prefix("X "),
            )
            .changed();
        changed |= ui
            .add(
                egui::DragValue::new(&mut layer.offset_world[1])
                    .speed(0.05)
                    .prefix("Y "),
            )
            .changed();
    });
    ui.horizontal(|ui| {
        ui.label("Motion");
        changed |= ui
            .add(
                egui::DragValue::new(&mut layer.motion_world_per_second[0])
                    .speed(0.01)
                    .prefix("X "),
            )
            .changed();
        changed |= ui
            .add(
                egui::DragValue::new(&mut layer.motion_world_per_second[1])
                    .speed(0.01)
                    .prefix("Y "),
            )
            .changed();
    });
    ui.small("0 parallax = screen-fixed · Motion uses world units/second.");
    changed
}

fn range_row(ui: &mut egui::Ui, label: &str, range: &mut [f32; 2], speed: f64) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        changed |= ui
            .add(
                egui::DragValue::new(&mut range[0])
                    .speed(speed)
                    .prefix("Min "),
            )
            .changed();
        changed |= ui
            .add(
                egui::DragValue::new(&mut range[1])
                    .speed(speed)
                    .prefix("Max "),
            )
            .changed();
    });
    changed
}

fn lerp_color32(bottom: [u8; 4], top: [u8; 4], t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let channel = |index: usize| {
        (bottom[index] as f32 + (top[index] as f32 - bottom[index] as f32) * t).round() as u8
    };
    Color32::from_rgba_unmultiplied(channel(0), channel(1), channel(2), channel(3))
}

fn min_y_screen(rect: Rect, t: f32) -> f32 {
    rect.bottom() - rect.height() * t
}

fn preview_repeat_radius(repeat: bool, viewport: f32, tile: f32) -> i32 {
    if !repeat {
        return 0;
    }
    ((viewport / tile).ceil() as i32 / 2 + 2).clamp(1, 8)
}

fn parallax_coverage_size(map_size: [f32; 2], camera_size: [f32; 2], parallax: f32) -> [f32; 2] {
    let p = parallax.clamp(0.0, 1.0);
    [
        camera_size[0] + p * (map_size[0] - camera_size[0]).max(0.0),
        camera_size[1] + p * (map_size[1] - camera_size[1]).max(0.0),
    ]
}

fn parallax_fill_size(mode: ParallaxFillMode, natural: [f32; 2], coverage: [f32; 2]) -> [f32; 2] {
    match mode {
        ParallaxFillMode::Natural | ParallaxFillMode::Repeat => natural,
        ParallaxFillMode::Stretch => coverage,
        ParallaxFillMode::Fit | ParallaxFillMode::Cover => {
            let sx = coverage[0] / natural[0];
            let sy = coverage[1] / natural[1];
            let scale = if mode == ParallaxFillMode::Fit {
                sx.min(sy)
            } else {
                sx.max(sy)
            };
            [natural[0] * scale, natural[1] * scale]
        }
    }
}

// Rendering parity keeps the map/camera contract explicit at this leaf helper.
#[allow(clippy::too_many_arguments)]
fn paint_parallax_preview(
    painter: &egui::Painter,
    layer: &ParallaxLayer,
    texture: &EnvironmentPreviewTexture,
    pixels_per_world_unit: f32,
    map_center: [f32; 2],
    map_size: [f32; 2],
    camera_center: [f32; 2],
    camera_size: [f32; 2],
    elapsed_seconds: f64,
    to_screen: &impl Fn([f32; 2]) -> Pos2,
) {
    let ppu = pixels_per_world_unit.max(f32::EPSILON);
    let natural_size = [
        texture.image_size_px[0] as f32 / ppu,
        texture.image_size_px[1] as f32 / ppu,
    ];
    if natural_size[0] <= 0.0 || natural_size[1] <= 0.0 {
        return;
    }

    let p = layer.parallax.clamp(0.0, 1.0);
    let coverage = parallax_coverage_size(map_size, camera_size, p);
    let size = parallax_fill_size(layer.fill_mode, natural_size, coverage);
    let animated_offset = layer.animated_offset_world(elapsed_seconds, size);
    let base = [
        camera_center[0] * (1.0 - p) + map_center[0] * p + animated_offset[0],
        camera_center[1] * (1.0 - p) + map_center[1] * p + animated_offset[1],
    ];
    let repeat = layer.fill_mode == ParallaxFillMode::Repeat;
    let x_radius = preview_repeat_radius(repeat && layer.repeat_x, camera_size[0], size[0]);
    let y_radius = preview_repeat_radius(repeat && layer.repeat_y, camera_size[1], size[1]);
    let alpha = (layer.opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    let mut copies = 0usize;
    for y in -y_radius..=y_radius {
        for x in -x_radius..=x_radius {
            if copies >= 64 {
                return;
            }
            copies += 1;
            let center = [base[0] + x as f32 * size[0], base[1] + y as f32 * size[1]];
            let left = center[0] - size[0] * 0.5;
            let top = center[1] + size[1] * 0.5;
            for chunk in &texture.chunks {
                let [cx, cy, cw, ch] = chunk.source_rect_px;
                let u0 = cx as f32 / texture.image_size_px[0] as f32;
                let u1 = (cx + cw) as f32 / texture.image_size_px[0] as f32;
                let v0 = cy as f32 / texture.image_size_px[1] as f32;
                let v1 = (cy + ch) as f32 / texture.image_size_px[1] as f32;
                let positions = [
                    to_screen([left + u0 * size[0], top - v1 * size[1]]),
                    to_screen([left + u1 * size[0], top - v1 * size[1]]),
                    to_screen([left + u1 * size[0], top - v0 * size[1]]),
                    to_screen([left + u0 * size[0], top - v0 * size[1]]),
                ];
                let uv = [
                    Pos2::new(0.0, 1.0),
                    Pos2::new(1.0, 1.0),
                    Pos2::new(1.0, 0.0),
                    Pos2::new(0.0, 0.0),
                ];
                paint_textured_quad(painter, &chunk.texture, positions, uv, alpha);
            }
        }
    }
}

// Cloud preview mirrors runtime placement inputs one-for-one.
#[allow(clippy::too_many_arguments)]
fn paint_cloud_field_preview(
    painter: &egui::Painter,
    field: &CloudFieldAuthoring,
    textures: &[EnvironmentPreviewTexture],
    pixels_per_world_unit: f32,
    map_center: [f32; 2],
    map_size: [f32; 2],
    camera_center: [f32; 2],
    camera_size: [f32; 2],
    elapsed_seconds: f64,
    to_screen: &impl Fn([f32; 2]) -> Pos2,
) {
    if textures.is_empty() {
        return;
    }
    let p = field.parallax.clamp(0.0, 1.0);
    let coverage = parallax_coverage_size(map_size, camera_size, p);
    let count = cloud_instance_count(field.density, coverage[0], camera_size[0]);
    let specs = cloud_instance_specs(
        field,
        textures.len(),
        cloud_field_seed(&field.id, MAP_LAB_CLOUD_PREVIEW_SEED),
        count,
    );
    let base = [
        camera_center[0] * (1.0 - p) + map_center[0] * p,
        camera_center[1] * (1.0 - p) + map_center[1] * p,
    ];
    let ppu = pixels_per_world_unit.max(f32::EPSILON);

    for cloud in specs {
        let Some(texture) = textures.get(cloud.asset_index) else {
            continue;
        };
        let size = [
            texture.image_size_px[0] as f32 / ppu * cloud.scale,
            texture.image_size_px[1] as f32 / ppu * cloud.scale,
        ];
        let x = wrap_cloud_center_preview(
            cloud.x_unit,
            cloud.speed_world_per_second,
            elapsed_seconds,
            coverage[0],
            size[0],
        );
        let center = [
            base[0] + x,
            base[1] + (cloud.height_unit - 0.5) * camera_size[1],
        ];
        let left = center[0] - size[0] * 0.5;
        let top = center[1] + size[1] * 0.5;
        let alpha = (cloud.opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
        for chunk in &texture.chunks {
            let [cx, cy, cw, ch] = chunk.source_rect_px;
            let u0 = cx as f32 / texture.image_size_px[0] as f32;
            let u1 = (cx + cw) as f32 / texture.image_size_px[0] as f32;
            let v0 = cy as f32 / texture.image_size_px[1] as f32;
            let v1 = (cy + ch) as f32 / texture.image_size_px[1] as f32;
            let positions = [
                to_screen([left + u0 * size[0], top - v1 * size[1]]),
                to_screen([left + u1 * size[0], top - v1 * size[1]]),
                to_screen([left + u1 * size[0], top - v0 * size[1]]),
                to_screen([left + u0 * size[0], top - v0 * size[1]]),
            ];
            let uv = [
                Pos2::new(0.0, 1.0),
                Pos2::new(1.0, 1.0),
                Pos2::new(1.0, 0.0),
                Pos2::new(0.0, 0.0),
            ];
            paint_textured_quad(painter, &chunk.texture, positions, uv, alpha);
        }
    }
}

fn wrap_cloud_center_preview(
    x_unit: f32,
    speed_world_per_second: f32,
    elapsed_seconds: f64,
    coverage_width: f32,
    sprite_width: f32,
) -> f32 {
    let travel_width = coverage_width + sprite_width.max(0.0);
    if !travel_width.is_finite() || travel_width <= f32::EPSILON {
        return 0.0;
    }
    let value = x_unit * travel_width + speed_world_per_second * elapsed_seconds as f32;
    value.rem_euclid(travel_width) - travel_width * 0.5
}

fn foothold_color(kind: FootholdKind) -> Color32 {
    match kind {
        FootholdKind::OneWay => Color32::from_rgb(70, 205, 255),
        FootholdKind::Solid => Color32::from_rgb(255, 105, 80),
    }
}

fn paint_foothold_path(
    painter: &egui::Painter,
    path: &FootholdPath,
    to_screen: &impl Fn([f32; 2]) -> Pos2,
    draft: bool,
) {
    let color = foothold_color(path.kind);
    let width = if draft { 2.0 } else { 3.0 };
    for pair in path.points.windows(2) {
        painter.line_segment(
            [to_screen(pair[0]), to_screen(pair[1])],
            Stroke::new(width, color),
        );
    }
    for point in &path.points {
        painter.circle_filled(to_screen(*point), if draft { 3.0 } else { 4.0 }, color);
    }
}

fn paint_draft_foothold(
    painter: &egui::Painter,
    points: &[[f32; 2]],
    kind: FootholdKind,
    to_screen: &impl Fn([f32; 2]) -> Pos2,
) {
    let path = FootholdPath {
        id: "draft".to_owned(),
        kind,
        drop_through: kind == FootholdKind::OneWay,
        points: points.to_vec(),
    };
    paint_foothold_path(painter, &path, to_screen, true);
}

struct PreviewTextureChunk {
    source_rect_px: [u32; 4],
    texture: TextureHandle,
}

struct EnvironmentPreviewTexture {
    chunks: Vec<PreviewTextureChunk>,
    image_size_px: [u32; 2],
}

type TextureRegions = HashMap<String, HashMap<[u32; 4], Vec<PreviewTextureChunk>>>;

fn load_textures(ctx: &egui::Context, document: &MapLabDocument) -> Result<TextureRegions, String> {
    let graphic = find_graphic_root(&document.sidecar_path)?;
    let max_texture_side = ctx.input(|input| input.raw.max_texture_side.unwrap_or(2048));
    let mut textures = HashMap::new();

    for asset in &document.presentation.assets {
        let source_rects: HashSet<[u32; 4]> = document
            .presentation
            .layers
            .iter()
            .flat_map(|layer| &layer.sprites)
            .filter(|sprite| sprite.asset_id == asset.id)
            .map(|sprite| sprite.source_rect_px)
            .collect();
        if source_rects.is_empty() {
            continue;
        }

        let path = graphic.join(&asset.source_path);
        let image = image::open(&path)
            .map_err(|error| format!("decode {}: {error}", path.display()))?
            .to_rgba8();
        if [image.width(), image.height()] != asset.image_size_px {
            return Err(format!(
                "{} dimensions changed since compile",
                path.display()
            ));
        }

        let mut regions = HashMap::new();
        for rect in source_rects {
            let [x, y, width, height] = rect;
            if width == 0
                || height == 0
                || x.checked_add(width)
                    .is_none_or(|right| right > image.width())
                || y.checked_add(height)
                    .is_none_or(|bottom| bottom > image.height())
            {
                return Err(format!(
                    "{} has invalid source rect [{x}, {y}, {width}, {height}]",
                    path.display()
                ));
            }

            let chunks = split_source_rect(rect, max_texture_side.max(1) as u32);
            if chunks.len() > 1
                && document.presentation.layers.iter().any(|layer| {
                    layer.sprites.iter().any(|sprite| {
                        sprite.asset_id == asset.id
                            && sprite.source_rect_px == rect
                            && sprite.transform != TileTransform::default()
                    })
                })
            {
                return Err(format!(
                    "{} oversized transformed source rect [{x}, {y}, {width}, {height}] is not supported by Map Lab preview",
                    path.display()
                ));
            }

            // egui can expose a smaller texture-side limit than the source art.
            // Keep canonical geometry unchanged and upload the source region as
            // multiple exact pixel chunks instead of resizing the artwork.
            let mut preview_chunks = Vec::with_capacity(chunks.len());
            for chunk in chunks {
                let [chunk_x, chunk_y, chunk_width, chunk_height] = chunk;
                let region =
                    image::imageops::crop_imm(&image, chunk_x, chunk_y, chunk_width, chunk_height)
                        .to_image();
                let color = egui::ColorImage::from_rgba_unmultiplied(
                    [chunk_width as usize, chunk_height as usize],
                    region.as_raw(),
                );
                let texture_name = format!(
                    "{}@{chunk_x},{chunk_y}:{chunk_width}x{chunk_height}",
                    asset.id
                );
                preview_chunks.push(PreviewTextureChunk {
                    source_rect_px: chunk,
                    texture: ctx.load_texture(texture_name, color, egui::TextureOptions::NEAREST),
                });
            }
            regions.insert(rect, preview_chunks);
        }
        textures.insert(asset.id.clone(), regions);
    }

    Ok(textures)
}

// The two maps are intentionally distinct caches: single-layer textures and cloud variant pools.
#[allow(clippy::type_complexity)]
fn load_environment_textures(
    ctx: &egui::Context,
    document: &MapLabDocument,
) -> Result<
    (
        HashMap<String, EnvironmentPreviewTexture>,
        HashMap<String, Vec<EnvironmentPreviewTexture>>,
    ),
    String,
> {
    let graphic = find_graphic_root(&document.sidecar_path)?;
    let max_texture_side = ctx.input(|input| input.raw.max_texture_side.unwrap_or(2048));
    let mut textures = HashMap::new();
    for (kind, layer) in document
        .environment
        .parallax_layers
        .iter()
        .map(|layer| ("background", layer))
        .chain(
            document
                .environment
                .foreground_layers
                .iter()
                .map(|layer| ("foreground", layer)),
        )
    {
        textures.insert(
            layer.id.clone(),
            load_environment_preview_texture(
                ctx,
                &graphic.join(&layer.asset_path),
                &format!("environment:{kind}:{}", layer.id),
                max_texture_side,
            )?,
        );
    }

    let mut cloud_textures = HashMap::new();
    for field in &document.environment.cloud_fields {
        let asset_paths = resolve_png_asset_folder(&graphic, &field.asset_folder)?;
        let mut variants = Vec::with_capacity(asset_paths.len());
        for (index, asset_path) in asset_paths.iter().enumerate() {
            variants.push(load_environment_preview_texture(
                ctx,
                &graphic.join(asset_path),
                &format!("cloud:{}:{index}", field.id),
                max_texture_side,
            )?);
        }
        cloud_textures.insert(field.id.clone(), variants);
    }
    Ok((textures, cloud_textures))
}

fn load_environment_preview_texture(
    ctx: &egui::Context,
    path: &Path,
    texture_key: &str,
    max_texture_side: usize,
) -> Result<EnvironmentPreviewTexture, String> {
    let image = image::open(path)
        .map_err(|error| format!("decode {}: {error}", path.display()))?
        .to_rgba8();
    let size = [image.width(), image.height()];
    let mut chunks = Vec::new();
    for rect in split_source_rect([0, 0, size[0], size[1]], max_texture_side.max(1) as u32) {
        let [x, y, width, height] = rect;
        let region = image::imageops::crop_imm(&image, x, y, width, height).to_image();
        let color = egui::ColorImage::from_rgba_unmultiplied(
            [width as usize, height as usize],
            region.as_raw(),
        );
        chunks.push(PreviewTextureChunk {
            source_rect_px: rect,
            texture: ctx.load_texture(
                format!("{texture_key}@{x},{y}:{width}x{height}"),
                color,
                egui::TextureOptions::LINEAR,
            ),
        });
    }
    Ok(EnvironmentPreviewTexture {
        chunks,
        image_size_px: size,
    })
}

fn find_graphic_root(sidecar: &Path) -> Result<PathBuf, String> {
    sidecar
        .ancestors()
        .map(|ancestor| ancestor.join("Graphic"))
        .find(|path| path.is_dir())
        .ok_or_else(|| format!("cannot locate Graphic/ above {}", sidecar.display()))
}

fn paint_sprite(
    painter: &egui::Painter,
    sprite: &PresentationSprite,
    layer_opacity: f32,
    textures: &TextureRegions,
    to_screen: &impl Fn([f32; 2]) -> Pos2,
) {
    let Some(chunks) = textures
        .get(&sprite.asset_id)
        .and_then(|regions| regions.get(&sprite.source_rect_px))
    else {
        return;
    };
    let alpha = (layer_opacity * sprite.opacity * 255.0).round() as u8;

    if chunks.len() == 1 && chunks[0].source_rect_px == sprite.source_rect_px {
        let texture = &chunks[0].texture;
        let [w, h] = sprite.size_world;
        let [cx, cy] = sprite.position_world;
        let positions = [
            to_screen([cx - w * 0.5, cy - h * 0.5]),
            to_screen([cx + w * 0.5, cy - h * 0.5]),
            to_screen([cx + w * 0.5, cy + h * 0.5]),
            to_screen([cx - w * 0.5, cy + h * 0.5]),
        ];
        let uv = transformed_uv(sprite.transform).map(|[u, v]| Pos2::new(u, v));
        paint_textured_quad(painter, texture, positions, uv, alpha);
        return;
    }

    if sprite.transform != TileTransform::default() {
        return;
    }

    let [source_x, source_y, source_width, source_height] = sprite.source_rect_px;
    let [world_width, world_height] = sprite.size_world;
    let [cx, cy] = sprite.position_world;
    let world_left = cx - world_width * 0.5;
    let world_top = cy + world_height * 0.5;

    for chunk in chunks {
        let [chunk_x, chunk_y, chunk_width, chunk_height] = chunk.source_rect_px;
        let u0 = (chunk_x - source_x) as f32 / source_width as f32;
        let u1 = (chunk_x + chunk_width - source_x) as f32 / source_width as f32;
        let v0 = (chunk_y - source_y) as f32 / source_height as f32;
        let v1 = (chunk_y + chunk_height - source_y) as f32 / source_height as f32;
        let left = world_left + u0 * world_width;
        let right = world_left + u1 * world_width;
        let top = world_top - v0 * world_height;
        let bottom = world_top - v1 * world_height;
        let positions = [
            to_screen([left, bottom]),
            to_screen([right, bottom]),
            to_screen([right, top]),
            to_screen([left, top]),
        ];
        let uv = [
            Pos2::new(0.0, 1.0),
            Pos2::new(1.0, 1.0),
            Pos2::new(1.0, 0.0),
            Pos2::new(0.0, 0.0),
        ];
        paint_textured_quad(painter, &chunk.texture, positions, uv, alpha);
    }
}

fn paint_textured_quad(
    painter: &egui::Painter,
    texture: &TextureHandle,
    positions: [Pos2; 4],
    uv: [Pos2; 4],
    alpha: u8,
) {
    let mut mesh = egui::Mesh::with_texture(texture.id());
    for index in 0..4 {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: positions[index],
            uv: uv[index],
            color: Color32::from_white_alpha(alpha),
        });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    painter.add(mesh);
}

fn split_source_rect(rect: [u32; 4], max_side: u32) -> Vec<[u32; 4]> {
    let [x, y, width, height] = rect;
    let mut chunks = Vec::new();
    let mut offset_y = 0;
    while offset_y < height {
        let chunk_height = (height - offset_y).min(max_side);
        let mut offset_x = 0;
        while offset_x < width {
            let chunk_width = (width - offset_x).min(max_side);
            chunks.push([x + offset_x, y + offset_y, chunk_width, chunk_height]);
            offset_x += chunk_width;
        }
        offset_y += chunk_height;
    }
    chunks
}

fn transformed_uv(transform: TileTransform) -> [[f32; 2]; 4] {
    [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]].map(|mut uv| {
        if transform.flip_diagonal {
            uv.swap(0, 1);
        }
        if transform.flip_horizontal {
            uv[0] = 1.0 - uv[0];
        }
        if transform.flip_vertical {
            uv[1] = 1.0 - uv[1];
        }
        uv
    })
}

fn map_covers_camera(world_size: [f32; 2]) -> bool {
    world_size[0] >= CAMERA_WIDTH_WU && world_size[1] >= CAMERA_HEIGHT_WU
}

fn world_rect(center: [f32; 2], size: [f32; 2], to_screen: &impl Fn([f32; 2]) -> Pos2) -> Rect {
    Rect::from_two_pos(
        to_screen([center[0] - size[0] * 0.5, center[1] + size[1] * 0.5]),
        to_screen([center[0] + size[0] * 0.5, center[1] - size[1] * 0.5]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foothold_categories_have_distinct_preview_colors() {
        assert_ne!(
            foothold_color(FootholdKind::OneWay),
            foothold_color(FootholdKind::Solid)
        );
    }

    #[test]
    fn oversized_preview_region_splits_without_resizing_pixels() {
        assert_eq!(
            split_source_rect([0, 0, 4644, 1080], 2048),
            vec![
                [0, 0, 2048, 1080],
                [2048, 0, 2048, 1080],
                [4096, 0, 548, 1080],
            ]
        );
    }

    #[test]
    fn camera_coverage_is_independent_from_ppu_standard() {
        assert!(!map_covers_camera([19.44, 10.8]));
        assert!(map_covers_camera([CAMERA_WIDTH_WU, CAMERA_HEIGHT_WU]));
    }

    #[test]
    fn map_switch_loads_the_other_map_and_drops_local_state() {
        struct Restore(Vec<(PathBuf, Vec<u8>)>);
        impl Drop for Restore {
            fn drop(&mut self) {
                for (path, bytes) in &self.0 {
                    let _ = std::fs::write(path, bytes);
                }
            }
        }
        let shared_maps = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content/shared/maps");
        let _restore = Restore(
            std::fs::read_dir(&shared_maps)
                .expect("shared maps")
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
                .map(|path| {
                    let bytes = std::fs::read(&path).expect("read runtime map");
                    (path, bytes)
                })
                .collect(),
        );
        let ctx = egui::Context::default();
        let mut app = MapLabApp::new(&ctx);
        let first = app
            .document
            .as_ref()
            .expect("default map")
            .source
            .id
            .clone();
        assert!(app.available_maps.len() >= 2);
        app.selected_placement = Some(99);
        app.selected_catalog = Some(3);
        app.selected_point = Some((1, 2));
        app.portal_brush = true;
        app.draft_points.push([1.0, 2.0]);

        let other = app
            .available_maps
            .iter()
            .find(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("map.map2"))
            })
            .expect("second authored map")
            .clone();
        app.request_map_switch(&ctx, other, false);
        let second = app
            .document
            .as_ref()
            .expect("switched map")
            .source
            .id
            .clone();
        assert_ne!(second, first);
        assert_eq!(second, "map.map2");
        assert!(app.selected_placement.is_none());
        assert!(app.selected_catalog.is_none());
        assert!(app.selected_point.is_none());
        assert!(!app.portal_brush);
        assert!(app.draft_points.is_empty());
        assert!(app.pending_map_switch.is_none());

        let back = app
            .available_maps
            .iter()
            .find(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("map.map1"))
            })
            .expect("map1")
            .clone();
        app.placements_dirty = true;
        app.request_map_switch(&ctx, back.clone(), false);
        assert_eq!(app.pending_map_switch.as_ref(), Some(&back));
        app.confirm_pending_switch(&ctx, MapSwitchChoice::Cancel);
        assert!(app.pending_map_switch.is_none());
        assert_eq!(app.document.as_ref().unwrap().source.id, second);
        assert!(app.placements_dirty);

        app.document.as_mut().unwrap().placements.push(Placement {
            id: "placement.mob_999".into(),
            kind: PlacementKind::Monster,
            content_authored: "monster.not_authored".into(),
            position: [0.0, 0.0],
            portal_link: None,
        });
        app.request_map_switch(&ctx, back, false);
        app.confirm_pending_switch(&ctx, MapSwitchChoice::SaveAndSwitch);
        assert_eq!(app.document.as_ref().unwrap().source.id, second);
        assert!(app.placements_dirty);
        assert!(app.pending_map_switch.is_some());
        assert!(app.status.contains("SAVE ERROR"));
    }

    #[test]
    fn map1_uses_locked_purgatory_standard_ppu() {
        assert_eq!(PURGATORY_STANDARD_PPU, 100.0);
    }

    #[test]
    fn camera_and_player_references_use_repo_owned_scale() {
        assert!((CAMERA_WIDTH_WU - 23.111_11).abs() < 1e-4);
        assert_eq!(CAMERA_HEIGHT_WU, 13.0);
        assert_eq!(PLAYER_REFERENCE_SIZE_WU, [0.92, 1.38]);
    }

    #[test]
    fn diagonal_transform_precedes_horizontal_and_vertical() {
        let transformed = transformed_uv(TileTransform {
            flip_horizontal: true,
            flip_vertical: false,
            flip_diagonal: true,
        });
        assert_eq!(transformed[0], [0.0, 0.0]);
        assert_eq!(transformed[2], [1.0, 1.0]);
    }
}
