use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let map = root.join("content/authoring/maps/map.map1.purgatory-map.json");
    println!("cargo:rerun-if-changed={}", map.display());
    let environment = root.join("content/authoring/maps/map.map1.environment.json");
    println!("cargo:rerun-if-changed={}", environment.display());
    let environment_bytes = fs::read(&environment)
        .unwrap_or_else(|error| panic!("read {}: {error}", environment.display()));
    let environment_authoring: purgatory_content::MapEnvironmentAuthoring =
        serde_json::from_slice(&environment_bytes)
            .unwrap_or_else(|error| panic!("parse {}: {error}", environment.display()));
    for field in &environment_authoring.cloud_fields {
        println!(
            "cargo:rerun-if-changed={}",
            root.join("Graphic").join(&field.asset_folder).display()
        );
    }
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
    let compiled_environment = purgatory_content::compile_map_environment(
        &environment_authoring,
        &root.join("Graphic"),
    )
    .unwrap_or_else(|error| panic!("PURGATORY environment compilation failed:\n{error}"));
    fs::write(
        output.join("map.map1.environment.json"),
        purgatory_content::serialize_map_environment_pretty(&compiled_environment)
            .expect("serialize map environment presentation"),
    )
    .expect("write compiled map environment presentation");
}
