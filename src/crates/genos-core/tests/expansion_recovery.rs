use genos_core::expansion::{ppf::ResourceKind, recover_pack};
use std::path::PathBuf;

fn research_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../docs/research")
}

#[test]
fn recovers_mini_installation_and_univ_project() {
    let root = research_root();
    let ppi = root.join("experiments/expansion-fixtures/mini-install.ppi");
    let n27 = root.join("Genos_InstrumentInfo.n27");
    let ppf = root.join("experiments/expansion-fixtures/univ-tiny.ppf");
    if !ppi.exists() || !n27.exists() || !ppf.exists() {
        return;
    }
    let report = recover_pack(Some(&ppi), Some(&n27), Some(&ppf)).unwrap();
    let inst = report.installation.expect("ppi");
    assert_eq!(inst.header.model.as_deref(), Some("Genos"));
    assert!(!inst.packs.is_empty());
    let info = report.instrument.expect("n27");
    assert_eq!(info.pack_install_data_format_type, 3);
    let proj = report.project.expect("ppf");
    assert!(proj.resources.iter().any(|r| r.kind == ResourceKind::Aiff));
    assert!(
        report
            .capabilities
            .iter()
            .any(|c| c.category == "ppf-waveforms")
    );
}
