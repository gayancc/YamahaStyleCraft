//! Controlled safety fixtures; these are not evidence of Yamaha interoperability.
use genos_core::expansion::{iff, ppf, ppi};
use std::fs;

fn chunk(id: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut bytes = id.to_vec();
    bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    bytes.extend_from_slice(payload);
    bytes.resize(bytes.len().next_multiple_of(4), 0);
    bytes
}

fn project() -> Vec<u8> {
    ppf::write_ppf(
        "Safety",
        &[ppf::PpfWriteResource {
            title: "metadata".into(),
            extension: "xml".into(),
            data: b"<original/>".to_vec(),
        }],
    )
    .unwrap()
}

#[test]
fn extraction_refuses_existing_files_and_source_aliases() {
    let dir = tempfile::tempdir().unwrap();
    let data = project();
    let pkg = ppf::parse_ppf(&data).unwrap();
    let existing = dir.path().join("keep.xml");
    fs::write(&existing, b"user content").unwrap();
    assert!(ppf::extract_resource(&data, &pkg.resources[0], dir.path(), "keep.xml").is_err());
    assert_eq!(fs::read(&existing).unwrap(), b"user content");
    let source = dir.path().join("source.ppf");
    fs::write(&source, &data).unwrap();
    assert!(ppf::extract_resource(&data, &pkg.resources[0], dir.path(), "source.ppf").is_err());
    assert_eq!(fs::read(&source).unwrap(), data);
    let out = ppf::extract_resource(&data, &pkg.resources[0], dir.path(), "fresh.xml").unwrap();
    assert_eq!(fs::read(out).unwrap(), b"<original/>");
}

#[test]
fn installation_extraction_refuses_existing_files() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("payload.bin");
    fs::write(&source, b"original").unwrap();
    let item = ppi::PackItem {
        title: None,
        extension: None,
        icon: None,
        fbin_kind: "binary".into(),
        fbin_size: 8,
        fbin_offset: 0,
        status: ppi::ContentStatus::PartiallyRecovered,
    };
    assert!(ppi::extract_pack_item(&source, &item, dir.path(), "payload.bin").is_err());
    assert_eq!(fs::read(source).unwrap(), b"original");
}

#[test]
fn ppf_rewrite_preserves_unknown_metadata_and_rejects_stale_package() {
    let mut data = project();
    data.extend(chunk(b"UNKN", b"vendor metadata"));
    let pkg = ppf::parse_ppf(&data).unwrap();
    assert_eq!(ppf::rewrite_ppf(&data, &pkg).unwrap(), data);
    let other = project();
    assert!(ppf::rewrite_ppf(&other, &pkg).is_err());
}

#[test]
fn deeply_nested_chunks_reject_without_unbounded_recursion() {
    let mut data = chunk(b"ETIT", b"leaf");
    for _ in 0..96 {
        data = chunk(b"BLOB", &data);
    }
    assert!(iff::parse_file(&data).is_err());
    // A normal nested container remains supported.
    assert!(iff::parse_file(&chunk(b"BLOB", &chunk(b"ETIT", b"leaf"))).is_ok());
}

#[test]
fn bulk_extraction_preserves_each_payload() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("bulk.ppf");
    let resources: Vec<_> = (0..64)
        .map(|i| ppf::PpfWriteResource {
            title: format!("resource{i}"),
            extension: "bin".into(),
            data: vec![i as u8; 8192],
        })
        .collect();
    fs::write(&source, ppf::write_ppf("Bulk", &resources).unwrap()).unwrap();
    let started = std::time::Instant::now();
    let result = genos_core::expansion::recovery::extract_all_resources(
        &genos_core::expansion::recovery::ExtractAllRequest {
            source_path: source.display().to_string(),
            dest_dir: dir.path().join("out").display().to_string(),
            kind: "ppf".into(),
        },
    )
    .unwrap();
    eprintln!("BULK_EXTRACTION_MS={}", started.elapsed().as_millis());
    assert_eq!(result.count, resources.len());
    for (out, resource) in result.paths.iter().zip(&resources) {
        assert_eq!(fs::read(out).unwrap(), resource.data);
    }
}

#[test]
fn oversized_instrument_info_is_rejected() {
    use std::io::{Seek, SeekFrom, Write};
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.seek(SeekFrom::Start(1024 * 1024)).unwrap();
    file.write_all(&[0]).unwrap();
    let error = genos_core::expansion::n27::parse_n27_file(file.path()).unwrap_err();
    assert!(error.to_string().contains("got 145"), "{error}");
}
