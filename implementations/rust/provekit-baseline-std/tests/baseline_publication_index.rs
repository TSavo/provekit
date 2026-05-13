// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CValue};
use provekit_verifier::cbor_decode::{decode, CborValue};
use serde_json::Value as Json;

const PROTOCOL_CATALOG_CID_V1_6_2: &str = "blake3-512:52bdb2be4b381cec2aff95db7755c84184878b45cd91882d262114a1abd2dd513f9ef3b250fb87093316fd0fcb48e4b97e109d463e57df5bda6aac0b1c719a0f";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn to_cvalue(value: &Json) -> std::sync::Arc<CValue> {
    match value {
        Json::Null => CValue::null(),
        Json::Bool(b) => CValue::boolean(*b),
        Json::Number(n) => CValue::integer(n.as_i64().expect("index numbers must be integers")),
        Json::String(s) => CValue::string(s.clone()),
        Json::Array(items) => CValue::array(items.iter().map(to_cvalue).collect()),
        Json::Object(map) => CValue::object(map.iter().map(|(k, v)| (k.clone(), to_cvalue(v)))),
    }
}

fn json_cid(value: &Json) -> String {
    let canonical = encode_jcs(&to_cvalue(value));
    blake3_512_of(canonical.as_bytes())
}

fn text_field<'a>(value: &'a Json, key: &str) -> &'a str {
    value
        .get(key)
        .and_then(Json::as_str)
        .unwrap_or_else(|| panic!("missing string field `{key}`"))
}

fn int_field(value: &Json, key: &str) -> u64 {
    value
        .get(key)
        .and_then(Json::as_u64)
        .unwrap_or_else(|| panic!("missing integer field `{key}`"))
}

fn catalog_field<'a>(
    root: &'a std::collections::BTreeMap<String, CborValue>,
    key: &str,
) -> &'a str {
    root.get(key)
        .and_then(CborValue::as_tstr)
        .unwrap_or_else(|| panic!("catalog missing text field `{key}`"))
}

fn expect_content_addressed_filename(path: &Path, cid: &str, suffix: &str) {
    let expected = format!("{cid}{suffix}");
    let actual = path.file_name().and_then(|s| s.to_str()).unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn baseline_publication_index_is_content_addressed_and_points_to_shipped_proofs() {
    let root = repo_root();
    let baseline_dir = root.join(".provekit/baselines");
    let mut index_paths: Vec<PathBuf> = fs::read_dir(&baseline_dir)
        .expect("read baseline dir")
        .map(|entry| entry.expect("baseline dir entry").path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".baseline-index.json"))
        })
        .collect();
    index_paths.sort();

    for proof_path in fs::read_dir(&baseline_dir)
        .expect("read baseline dir")
        .map(|entry| entry.expect("baseline dir entry").path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("proof"))
    {
        let file_name = proof_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap();
        assert!(
            file_name.starts_with("blake3-512:"),
            "friendly proof aliases are not canonical publications: {file_name}"
        );
    }

    assert_eq!(
        index_paths.len(),
        1,
        "expected exactly one published baseline index"
    );

    let index_bytes = fs::read(&index_paths[0]).expect("read baseline index");
    let index: Json = serde_json::from_slice(&index_bytes).expect("parse baseline index");
    let index_cid = json_cid(&index);
    expect_content_addressed_filename(&index_paths[0], &index_cid, ".baseline-index.json");

    assert_eq!(
        text_field(&index, "kind"),
        "provekit.baseline.publication_index"
    );
    assert_eq!(int_field(&index, "schema_version"), 1);
    assert_eq!(text_field(&index, "protocol_version"), "v1.6.2");
    assert_eq!(
        text_field(&index, "protocol_catalog_cid"),
        PROTOCOL_CATALOG_CID_V1_6_2
    );
    assert_eq!(text_field(&index, "friendly_alias_policy"), "none");
    assert_eq!(
        index.pointer("/lsp_callsite_index/status"),
        Some(&Json::String("content-addressed-artifact".into()))
    );
    assert_eq!(
        index.pointer("/lsp_callsite_index/compatibility_aliases"),
        Some(&Json::Array(Vec::new()))
    );

    let entries = index
        .get("entries")
        .and_then(Json::as_array)
        .expect("entries array");
    assert_eq!(entries.len(), 12);

    let mut languages = BTreeSet::new();
    for entry in entries {
        let language = text_field(entry, "language");
        languages.insert(language.to_string());

        assert_eq!(text_field(entry, "status"), "shipped");
        assert_eq!(text_field(entry, "signer_role"), "foundation-baseline");
        assert!(!text_field(entry, "authored_against").is_empty());

        let proof_cid = text_field(entry, "proof_cid");
        let proof_rel = text_field(entry, "proof_path");
        assert_eq!(proof_rel, format!(".provekit/baselines/{proof_cid}.proof"));
        assert!(!proof_rel.contains(".idx"));

        let proof_path = root.join(proof_rel);
        expect_content_addressed_filename(&proof_path, proof_cid, ".proof");

        let proof_bytes = fs::read(&proof_path)
            .unwrap_or_else(|e| panic!("read proof file {}: {e}", proof_path.display()));
        assert_eq!(blake3_512_of(&proof_bytes), proof_cid);

        let catalog = decode(&proof_bytes).expect("decode proof CBOR");
        let catalog_root = catalog.as_map().expect("proof catalog root");
        assert_eq!(
            catalog_field(catalog_root, "name"),
            text_field(entry, "baseline_name")
        );
        assert_eq!(
            catalog_field(catalog_root, "version"),
            text_field(entry, "baseline_version")
        );
        assert_eq!(
            catalog_field(catalog_root, "signer"),
            text_field(entry, "signer")
        );
        assert_eq!(
            catalog_field(catalog_root, "declaredAt"),
            text_field(entry, "declared_at")
        );

        let member_count = catalog_root
            .get("members")
            .and_then(CborValue::as_map)
            .expect("catalog members")
            .len() as u64;
        assert_eq!(member_count, int_field(entry, "member_count"));
    }

    assert_eq!(
        languages,
        BTreeSet::from([
            "c".to_string(),
            "cpp".to_string(),
            "csharp".to_string(),
            "go".to_string(),
            "java".to_string(),
            "php".to_string(),
            "python".to_string(),
            "ruby".to_string(),
            "rust".to_string(),
            "swift".to_string(),
            "typescript".to_string(),
            "zig".to_string(),
        ])
    );
}
