//! Tests for `stage::loader` against synthetic DAT files.
//!
//! The compiler-generated fixture is covered by the integration test in
//! `tests/stage_loader_fixture.rs`.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::stage::dat::error::{DecodeError, HeaderError, SectionError, WorldError};
use crate::stage::dat::flags::{
    HEADER_FLAG_HAS_CUSTOM_ASSETS, HEADER_FLAG_HAS_DOCUMENTS, HEADER_FLAG_HAS_LOCALIZATION,
    HEADER_FLAG_HAS_WORLD,
};
use crate::stage::dat::section_type::SectionType;
use crate::stage::error::StageLoadError;
use crate::stage::loader::load_from_bytes;

use super::support::{SectionSpec, build_dat, build_world};

fn meta_json() -> Value {
    json!({
        "format": "OOPSMDAT",
        "format_version": 1,
        "schema_version": 1,
        "compiler": "tests/1.0.0",
        "stage_id": "test_stage",
        "title": "stage.test_stage.title",
        "grade_min": 5,
        "grade_max": 6,
        "topics": ["algebra"],
        "locales": ["en-US"],
        "sections": ["META", "STAG"],
        "content_sha256": "",
    })
}

fn stag_json() -> Value {
    json!({
        "schema_version": 1,
        "stage": { "id": "test_stage" },
        "learning": {},
        "world": {},
        "construction": {},
        "objectives": {},
        "rewards": {},
        "extensions": { "future": true },
    })
}

fn msgpack(value: &Value) -> Vec<u8> {
    rmp_serde::to_vec_named(value).expect("serialize MessagePack")
}

fn core_sections() -> Vec<SectionSpec> {
    vec![
        SectionSpec::new(b"META", msgpack(&meta_json())),
        SectionSpec::new(b"STAG", msgpack(&stag_json())),
    ]
}

fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn load_with(header_flags: u32, extra: Vec<SectionSpec>) -> Result<crate::stage::StagePackage, StageLoadError> {
    let mut sections = core_sections();
    sections.extend(extra);
    load_from_bytes(&build_dat(header_flags, &sections))
}

// -----------------------------------------------------------------------------
// Successful loads
// -----------------------------------------------------------------------------

#[test]
fn loads_minimal_package() {
    let package = load_from_bytes(&build_dat(0, &core_sections())).unwrap();
    assert_eq!(package.meta.stage_id, "test_stage");
    assert_eq!(package.meta.grade_min, 5);
    assert_eq!(package.meta.description, None);
    assert_eq!(package.stage.schema_version, 1);
    assert_eq!(package.stage.stage["id"], "test_stage");
    assert_eq!(package.stage.extra["extensions"]["future"], true);
    assert!(package.world.is_none());
    assert!(package.localization.is_empty());
    assert!(package.documents.is_empty());
    assert!(package.custom_assets.is_empty());
}

#[test]
fn loads_zstd_compressed_sections() {
    let sections = vec![
        SectionSpec::new(b"META", msgpack(&meta_json())).zstd(),
        SectionSpec::new(b"STAG", msgpack(&stag_json())).zstd(),
    ];
    let package = load_from_bytes(&build_dat(0, &sections)).unwrap();
    assert_eq!(package.meta.stage_id, "test_stage");
}

#[test]
fn loads_world_section() {
    let mut dense = vec![0u8; 4096];
    dense[0] = 1;
    let world = build_world([16, 16, 16], &["air", "brick"], vec![(0, 0, 0, 0, dense)]);
    let package = load_with(HEADER_FLAG_HAS_WORLD, vec![SectionSpec::new(b"WRLD", world)]).unwrap();
    let world = package.world.expect("world section was present");
    assert_eq!(world.block_name([0, 0, 0]), Some("brick"));
}

#[test]
fn loads_localization_sorted_by_locale() {
    let mut files: BTreeMap<String, serde_bytes::ByteBuf> = BTreeMap::new();
    files.insert("fa".into(), serde_bytes::ByteBuf::from(b"title = salam".to_vec()));
    files.insert("en-US".into(), serde_bytes::ByteBuf::from(b"title = hello".to_vec()));
    let locl = rmp_serde::to_vec(&files).unwrap();

    let package = load_with(HEADER_FLAG_HAS_LOCALIZATION, vec![SectionSpec::new(b"LOCL", locl)]).unwrap();
    let locales: Vec<&str> = package.localization.iter().map(|l| l.locale.as_str()).collect();
    assert_eq!(locales, ["en-US", "fa"]);
    assert_eq!(
        package.localization_for("fa").unwrap().raw,
        b"title = salam".to_vec()
    );
}

#[test]
fn loads_documents() {
    let docs = json!({
        "version": 1,
        "documents": [
            { "id": "lesson.md", "mime": "text/markdown", "text": "# Lesson" },
            { "id": "solution.md", "mime": "text/markdown", "text": "# Solution" },
        ],
    });
    let package = load_with(
        HEADER_FLAG_HAS_DOCUMENTS,
        vec![SectionSpec::new(b"DOCS", msgpack(&docs))],
    )
    .unwrap();
    assert_eq!(package.documents.len(), 2);
    assert_eq!(package.document("solution.md").unwrap().text, "# Solution");
    assert!(package.document("missing.md").is_none());
}

#[test]
fn loads_custom_assets() {
    let blob = b"glb-bytes-one|glb-bytes-two".to_vec();
    let index = json!({
        "version": 1,
        "assets": [
            {
                "id": "first", "type": "model", "mime": "model/gltf-binary",
                "offset": 0, "size": 13, "sha256": sha256_hex(&blob[..13]),
            },
            {
                "id": "second", "type": "model", "mime": "model/gltf-binary",
                "offset": 14, "size": 13,
            },
        ],
    });
    let package = load_with(
        HEADER_FLAG_HAS_CUSTOM_ASSETS,
        vec![
            SectionSpec::new(b"ASIX", msgpack(&index)),
            SectionSpec::new(b"ASDT", blob),
        ],
    )
    .unwrap();

    assert_eq!(package.custom_assets.len(), 2);
    assert_eq!(package.custom_asset("first").unwrap().data, b"glb-bytes-one");
    let second = package.custom_asset("second").unwrap();
    assert_eq!(second.data, b"glb-bytes-two");
    assert_eq!(second.sha256_hex, "");
}

// -----------------------------------------------------------------------------
// Rejections
// -----------------------------------------------------------------------------

#[test]
fn rejects_empty_and_zeroed_input() {
    assert!(matches!(
        load_from_bytes(&[]),
        Err(StageLoadError::Header(HeaderError::HeaderTooSmall { actual: 0 }))
    ));
    assert!(matches!(
        load_from_bytes(&[0u8; 64]),
        Err(StageLoadError::Header(HeaderError::InvalidMagic))
    ));
}

#[test]
fn rejects_header_flag_without_section() {
    let err = load_with(HEADER_FLAG_HAS_LOCALIZATION, vec![]).unwrap_err();
    assert!(matches!(
        err,
        StageLoadError::Section(SectionError::MissingRequiredSection(SectionType::LOCL))
    ));
}

#[test]
fn rejects_wrong_meta_format() {
    let mut meta = meta_json();
    meta["format"] = json!("NOTADAT");
    let sections = vec![
        SectionSpec::new(b"META", msgpack(&meta)),
        SectionSpec::new(b"STAG", msgpack(&stag_json())),
    ];
    let err = load_from_bytes(&build_dat(0, &sections)).unwrap_err();
    assert!(matches!(
        err,
        StageLoadError::MetaFormatMismatch { ref declared } if declared == "NOTADAT"
    ));
}

#[test]
fn rejects_meta_that_disagrees_with_header() {
    let mut meta = meta_json();
    meta["schema_version"] = json!(2);
    let sections = vec![
        SectionSpec::new(b"META", msgpack(&meta)),
        SectionSpec::new(b"STAG", msgpack(&stag_json())),
    ];
    let err = load_from_bytes(&build_dat(0, &sections)).unwrap_err();
    assert!(matches!(
        err,
        StageLoadError::MetaHeaderMismatch {
            field: "schema_version",
            meta: 2,
            header: 1,
        }
    ));
}

#[test]
fn rejects_meta_that_is_not_msgpack() {
    // 0xC1 is the one byte value MessagePack never uses.
    let sections = vec![
        SectionSpec::new(b"META", vec![0xC1]),
        SectionSpec::new(b"STAG", msgpack(&stag_json())),
    ];
    let err = load_from_bytes(&build_dat(0, &sections)).unwrap_err();
    assert!(matches!(
        err,
        StageLoadError::Decode(DecodeError::InvalidMsgpack {
            section: SectionType::META,
            ..
        })
    ));
}

#[test]
fn rejects_stage_missing_required_object() {
    let mut stag = stag_json();
    stag.as_object_mut().unwrap().remove("rewards");
    let sections = vec![
        SectionSpec::new(b"META", msgpack(&meta_json())),
        SectionSpec::new(b"STAG", msgpack(&stag)),
    ];
    let err = load_from_bytes(&build_dat(0, &sections)).unwrap_err();
    assert!(matches!(
        err,
        StageLoadError::Decode(DecodeError::InvalidMsgpackStructure {
            section: SectionType::STAG,
            ..
        })
    ));
}

#[test]
fn rejects_documents_payload_without_documents() {
    let docs = json!({ "version": 1 });
    let err = load_with(
        HEADER_FLAG_HAS_DOCUMENTS,
        vec![SectionSpec::new(b"DOCS", msgpack(&docs))],
    )
    .unwrap_err();
    assert!(matches!(
        err,
        StageLoadError::Decode(DecodeError::InvalidMsgpackStructure {
            section: SectionType::DOCS,
            ..
        })
    ));
}

#[test]
fn rejects_corrupt_world() {
    let err = load_with(
        HEADER_FLAG_HAS_WORLD,
        vec![SectionSpec::new(b"WRLD", vec![0u8; 40])],
    )
    .unwrap_err();
    assert!(matches!(
        err,
        StageLoadError::Decode(DecodeError::InvalidWorld(WorldError::InvalidMagic))
    ));
}

#[test]
fn rejects_asset_outside_asdt() {
    let index = json!({
        "assets": [{
            "id": "big", "type": "model", "mime": "model/gltf-binary",
            "offset": 0, "size": 100,
        }],
    });
    let err = load_with(
        HEADER_FLAG_HAS_CUSTOM_ASSETS,
        vec![
            SectionSpec::new(b"ASIX", msgpack(&index)),
            SectionSpec::new(b"ASDT", vec![0u8; 9]),
        ],
    )
    .unwrap_err();
    assert!(matches!(err, StageLoadError::Asset { .. }));
}

#[test]
fn rejects_asset_range_overflow() {
    let index = json!({
        "assets": [{
            "id": "wrap", "type": "model", "mime": "model/gltf-binary",
            "offset": u64::MAX, "size": 2,
        }],
    });
    let err = load_with(
        HEADER_FLAG_HAS_CUSTOM_ASSETS,
        vec![SectionSpec::new(b"ASIX", msgpack(&index))],
    )
    .unwrap_err();
    assert!(matches!(err, StageLoadError::Asset { .. }));
}

#[test]
fn rejects_asset_hash_mismatch() {
    let index = json!({
        "assets": [{
            "id": "tampered", "type": "model", "mime": "model/gltf-binary",
            "offset": 0, "size": 4, "sha256": sha256_hex(b"something else"),
        }],
    });
    let err = load_with(
        HEADER_FLAG_HAS_CUSTOM_ASSETS,
        vec![
            SectionSpec::new(b"ASIX", msgpack(&index)),
            SectionSpec::new(b"ASDT", b"data".to_vec()),
        ],
    )
    .unwrap_err();
    assert!(matches!(
        err,
        StageLoadError::Asset {
            section: SectionType::ASDT,
            ..
        }
    ));
}
