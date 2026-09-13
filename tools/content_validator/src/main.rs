fn main() {
    let root = purgatory_content::default_content_root();
    match purgatory_content::load_registry(&root, purgatory_content::LoadMode::Full) {
        Ok(registry) => {
            println!(
                "PURGATORY content validator OK root={} maps={} entities={} monsters={} equipment={} defs={}",
                root.display(),
                registry.map_count(),
                registry.entity_count(),
                registry.monster_count(),
                registry.equipment_count(),
                registry.definition_count()
            );
        }
        Err(err) => {
            eprintln!("PURGATORY content validator FAILED\n{err}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn workspace_crates_are_linked() {
        assert!(!purgatory_common::version().is_empty());
        assert!(!purgatory_content::version().is_empty());
    }

    #[test]
    fn workspace_pack_validates() {
        let root = purgatory_content::default_content_root();
        let registry = purgatory_content::load_registry(&root, purgatory_content::LoadMode::Full)
            .expect("workspace content");
        assert!(registry.map_count() >= 2);
        assert!(registry.equipment_count() >= 8);
        assert_eq!(registry.monster_count(), 1);
    }
}
