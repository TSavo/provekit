// SPDX-License-Identifier: Apache-2.0

use std::fs;

use provekit_cli::kit_dispatch::{
    configured_exam_manifest_cid, federate_plugin_registries, seal_plugin_registry_for_project,
    KitDispatchError, DEFAULT_EXAM_MANIFEST_CID, EXAM_MANIFEST_MISMATCH_REASON,
};
use provekit_plugin_loader::registry::PluginRegistry;

const OTHER_EXAM_MANIFEST_CID: &str = "blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111";

#[test]
fn registry_memento_serializes_optional_exam_manifest_fields() {
    let registry = PluginRegistry::new();

    let memento = registry.emit_registry_memento_with_exam_manifest(
        "2026-05-17T00:00:00.000Z",
        Some(DEFAULT_EXAM_MANIFEST_CID.to_string()),
        Some(vec![
            DEFAULT_EXAM_MANIFEST_CID.to_string(),
            OTHER_EXAM_MANIFEST_CID.to_string(),
        ]),
    );

    assert_eq!(
        memento.header.exam_manifest_cid.as_deref(),
        Some(DEFAULT_EXAM_MANIFEST_CID)
    );
    // exam_manifest_set is sorted lex-ascending for canonical CID determinism
    // (federation requires byte-identical bytes regardless of caller insertion order).
    let mut expected_set = vec![
        DEFAULT_EXAM_MANIFEST_CID.to_string(),
        OTHER_EXAM_MANIFEST_CID.to_string(),
    ];
    expected_set.sort();
    assert_eq!(
        memento.header.exam_manifest_set.as_deref(),
        Some(&expected_set[..])
    );

    let json = serde_json::to_string(&memento).expect("serialize registry memento");
    assert!(json.contains("\"exam_manifest_cid\""));
    assert!(json.contains("\"exam_manifest_set\""));

    let legacy = registry.emit_registry_memento("2026-05-17T00:00:00.000Z");
    let legacy_json = serde_json::to_string(&legacy).expect("serialize legacy registry memento");
    assert!(!legacy_json.contains("exam_manifest_cid"));
    assert!(!legacy_json.contains("exam_manifest_set"));
}

#[test]
fn configured_exam_manifest_cid_defaults_to_locked_v1_manifest() {
    let workspace = tempfile::tempdir().expect("tempdir");

    assert_eq!(
        configured_exam_manifest_cid(workspace.path()),
        DEFAULT_EXAM_MANIFEST_CID
    );
}

#[test]
fn configured_exam_manifest_cid_reads_project_config() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let config_dir = workspace.path().join(".provekit");
    fs::create_dir_all(&config_dir).expect("create config dir");
    fs::write(
        config_dir.join("config.toml"),
        format!("exam_manifest_cid = \"{OTHER_EXAM_MANIFEST_CID}\"\n"),
    )
    .expect("write config");

    assert_eq!(
        configured_exam_manifest_cid(workspace.path()),
        OTHER_EXAM_MANIFEST_CID
    );
}

#[test]
fn seal_plugin_registry_for_project_populates_exam_manifest_cid_from_config() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let config_dir = workspace.path().join(".provekit");
    fs::create_dir_all(&config_dir).expect("create config dir");
    fs::write(
        config_dir.join("config.toml"),
        format!("exam_manifest_cid = \"{OTHER_EXAM_MANIFEST_CID}\"\n"),
    )
    .expect("write config");

    let registry = PluginRegistry::new();
    let memento =
        seal_plugin_registry_for_project(&registry, workspace.path(), "2026-05-17T00:00:00.000Z");

    assert_eq!(
        memento.header.exam_manifest_cid.as_deref(),
        Some(OTHER_EXAM_MANIFEST_CID)
    );
}

#[test]
fn same_manifest_cid_federates_successfully() {
    let registry = PluginRegistry::new();
    let local = registry.emit_registry_memento_with_exam_manifest(
        "2026-05-17T00:00:00.000Z",
        Some(DEFAULT_EXAM_MANIFEST_CID.to_string()),
        None,
    );
    let remote = registry.emit_registry_memento_with_exam_manifest(
        "2026-05-17T00:00:00.000Z",
        Some(DEFAULT_EXAM_MANIFEST_CID.to_string()),
        None,
    );

    federate_plugin_registries(&local, &remote).expect("same manifest CID federates");
}

#[test]
fn different_manifest_cid_refuses_with_exam_manifest_mismatch() {
    let registry = PluginRegistry::new();
    let local = registry.emit_registry_memento_with_exam_manifest(
        "2026-05-17T00:00:00.000Z",
        Some(DEFAULT_EXAM_MANIFEST_CID.to_string()),
        None,
    );
    let remote = registry.emit_registry_memento_with_exam_manifest(
        "2026-05-17T00:00:00.000Z",
        Some(OTHER_EXAM_MANIFEST_CID.to_string()),
        None,
    );

    let error = federate_plugin_registries(&local, &remote).expect_err("mismatch must refuse");

    assert_eq!(
        error,
        KitDispatchError::ExamManifestMismatch {
            local_manifest_cid: DEFAULT_EXAM_MANIFEST_CID.to_string(),
            remote_manifest_cid: OTHER_EXAM_MANIFEST_CID.to_string(),
        }
    );
    assert_eq!(error.refused_reason(), EXAM_MANIFEST_MISMATCH_REASON);
}

#[test]
fn legacy_registry_without_manifest_federates_with_itself() {
    let registry = PluginRegistry::new();
    let legacy = registry.emit_registry_memento("2026-05-17T00:00:00.000Z");

    federate_plugin_registries(&legacy, &legacy).expect("legacy self federation passes");
}

#[test]
fn exam_manifest_mismatch_refusal_payload_has_required_shape() {
    let error = KitDispatchError::ExamManifestMismatch {
        local_manifest_cid: DEFAULT_EXAM_MANIFEST_CID.to_string(),
        remote_manifest_cid: OTHER_EXAM_MANIFEST_CID.to_string(),
    };

    let payload = error.refusal_payload();

    assert_eq!(
        payload["refused_reason"].as_str(),
        Some(EXAM_MANIFEST_MISMATCH_REASON)
    );
    assert_eq!(
        payload["local_manifest_cid"].as_str(),
        Some(DEFAULT_EXAM_MANIFEST_CID)
    );
    assert_eq!(
        payload["remote_manifest_cid"].as_str(),
        Some(OTHER_EXAM_MANIFEST_CID)
    );
    assert_eq!(payload.as_object().expect("payload is object").len(), 3);
}

#[test]
fn exam_manifest_mismatch_discriminates_from_equal_manifests() {
    let registry = PluginRegistry::new();
    let local = registry.emit_registry_memento_with_exam_manifest(
        "2026-05-17T00:00:00.000Z",
        Some(OTHER_EXAM_MANIFEST_CID.to_string()),
        None,
    );
    let remote = registry.emit_registry_memento_with_exam_manifest(
        "2026-05-17T00:00:00.000Z",
        Some(OTHER_EXAM_MANIFEST_CID.to_string()),
        None,
    );

    assert!(federate_plugin_registries(&local, &remote).is_ok());
}
