use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let map = root.join("Graphic/assets/maps/map1.tmx");
    println!("cargo:rerun-if-changed={}", map.display());
    println!(
        "cargo:rerun-if-changed={}",
        map.with_file_name("TEST.tsx").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        map.with_file_name("BG.png").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        map.with_file_name("WelcomeGrass_Terrain_Tileset.png")
            .display()
    );
    let presentation =
        purgatory_content::compile_tiled_map(&map, purgatory_content::TILED_PIXELS_PER_WORLD_UNIT)
            .unwrap_or_else(|error| panic!("PURGATORY Tiled map compilation failed:\n{error}"));
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    fs::write(
        output.join("map.dev.footnote.presentation.json"),
        serde_json::to_vec_pretty(&presentation).expect("serialize map presentation"),
    )
    .expect("write compiled map presentation");
}
