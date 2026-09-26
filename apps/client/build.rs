use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let map = root.join("content/authoring/maps/map.map1.purgatory-map.json");
    println!("cargo:rerun-if-changed={}", map.display());
    println!(
        "cargo:rerun-if-changed={}",
        root.join("Graphic/assets/maps/map1.tmx").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        root.join("Graphic/assets/maps/TEST.tsx").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        root.join("Graphic/assets/maps/BG.png").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        root.join("Graphic/assets/maps/WelcomeGrass_Terrain_Tileset.png")
            .display()
    );
    let presentation = purgatory_content::compile_tiled_map(&map)
        .unwrap_or_else(|error| panic!("PURGATORY Tiled map compilation failed:\n{error}"));
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    fs::write(
        output.join("map.map1.presentation.json"),
        purgatory_content::serialize_map_pretty(&presentation).expect("serialize map presentation"),
    )
    .expect("write compiled map presentation");
}
