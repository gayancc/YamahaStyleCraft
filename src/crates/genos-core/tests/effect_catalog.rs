use genos_core::effect_catalog;
use std::{collections::BTreeMap, path::PathBuf, sync::OnceLock};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../docs/development")
}

fn catalog() -> effect_catalog::Catalog {
    static C: OnceLock<effect_catalog::Catalog> = OnceLock::new();
    C.get_or_init(|| {
        effect_catalog::import_pdf(
            &std::fs::read(fixtures().join("voice-catalog/genos_en_dl_f0.pdf")).unwrap(),
            "genos_en_dl_f0.pdf",
        )
        .unwrap()
    })
    .clone()
}

#[test]
fn effect_type_list_matches_oracle_addresses_names_and_blocks() {
    let c = catalog();
    let expected: Vec<serde_json::Value> = serde_json::from_slice(
        &std::fs::read(fixtures().join("cursor-followup/oracle-effects.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(c.effects.len(), 528);
    assert_eq!(expected.len(), 528);
    let s = c.summary();
    assert_eq!((s.reverb, s.chorus, s.variation), (60, 108, 360));
    let got: BTreeMap<_, _> = c
        .effects
        .iter()
        .map(|e| ((e.block.clone(), e.msb, e.lsb), e))
        .collect();
    for exp in &expected {
        let key = (
            exp["block"].as_str().unwrap().to_string(),
            exp["msb"].as_u64().unwrap() as u8,
            exp["lsb"].as_u64().unwrap() as u8,
        );
        let e = got.get(&key).unwrap_or_else(|| panic!("missing {key:?}"));
        assert_eq!(e.category, exp["category"].as_str().unwrap());
        assert_eq!(e.name, exp["name"].as_str().unwrap());
        assert_eq!(
            e.description,
            exp["description"].as_str().unwrap(),
            "description for {key:?}"
        );
        assert_eq!(e.parameter_group, exp["parameterGroup"].as_str().unwrap());
        assert_eq!(e.page, exp["page"].as_u64().unwrap() as u32);
    }
}

#[test]
fn cache_integrity_and_failed_import_preserve_existing_bytes() {
    let c = catalog();
    let dir = std::env::temp_dir().join(format!("gss-effects-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("effect-catalog.json");
    effect_catalog::save(&path, &c).unwrap();
    let before = std::fs::read(&path).unwrap();
    assert_eq!(
        effect_catalog::load(&path).unwrap().unwrap().effects,
        c.effects
    );
    assert!(effect_catalog::import_pdf(b"%PDF-bad", "bad.pdf").is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let mut invalid = c.clone();
    invalid.effects[0].msb = 128;
    assert!(effect_catalog::save(&path, &invalid).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let mut changed = c;
    changed.effects[0].name.push_str(" changed");
    std::fs::write(&path, serde_json::to_vec(&changed).unwrap()).unwrap();
    assert!(effect_catalog::load(&path).is_err());
    std::fs::remove_file(&path).unwrap();
    std::fs::remove_dir(&dir).unwrap();
}

#[test]
fn effect_search_filters_by_block_and_terms() {
    let c = catalog();
    let halls = c.search("Real Large Hall", Some("reverb")).unwrap();
    assert!(halls.effects.iter().any(|e| e.name == "Real Large Hall"));
    assert!(halls.effects.iter().all(|e| e.block == "reverb"));
    let ambience = c.search("Ambience Celeste", Some("variation")).unwrap();
    assert!(
        ambience
            .effects
            .iter()
            .any(|e| e.name == "Ambience Celeste")
    );
    assert_eq!(
        c.search("Ambience Celeste", Some("reverb")).unwrap().total,
        0
    );
    let all = c.search("", None).unwrap();
    assert_eq!(all.total, 528);
    assert_eq!(all.effects.len(), 100);
    assert!(c.search(&"x".repeat(121), None).is_err());
}
