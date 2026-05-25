// SPDX-License-Identifier: Apache-2.0
//
// REGRESSION COVERAGE for the rust-side fan-out of the `.proof`-load-via-RPC
// migration (#1458's java SugarRealizer change, mirrored for rust in the
// `feat(realize-rust): .proof-load-via-RPC` commit).
//
// WHAT THIS PROVES
// ----------------
// The four `rust-canonical-bodies-{serde_json,blake3,std::io,libprovekit-rpc-
// cross-platform}.json` disk caches were DELETED. For the three with a native
// `#[provekit::sugar]` rust shim, this test proves the deletion is safe: every
// concept those disk JSONs carried (and the real rust body for it) lives in the
// shim's signed `.proof` envelope, readable via the EXACT decode path
// `cmd_materialize::body_templates_from_shim_proof` /
// `augment_spec_with_shim_term_shape` use (cbor_decode -> members -> bstr ->
// utf8 -> serde_json -> `library-sugar-binding-entry`). The shim source is the
// content-addressed authority; the disk JSON was a redundant projection.
//
// It also pins the deletion as an invariant: `assert_orphan_disk_caches_gone`
// fails loudly if any of the four stale JSONs is re-introduced — the same
// invariant shape `cmd_materialize_proof_load.rs::assert_no_canonical_bodies_on_disk`
// uses for the java jackson/gson caches.
//
// WHAT THIS DOES *NOT* PROVE (open gap, separate enabler — NOT this branch)
// ------------------------------------------------------------------------
// It does NOT exercise an end-to-end `materialize --library <tag>` for these
// rust shims, because the rust prefer-RPC body-supply path is wired in the kit
// (`operator_body_template_for` consumes RPC `bodyTemplates` first) but is not
// yet connected to a live route for these three shims:
//   1. The rust shim realize manifests (`.provekit/realize/rust-shim-*`) declare
//      no `sugar_proof` pointer, so `body_templates_from_shim_proof` returns
//      empty (the java manifests DO declare it — that is why the java template
//      test can run end-to-end).
//   2. The shim `.proof` binding entries carry `target_library_tag = "serde_json"`
//      / `"blake3"` / `"std::io"` (the shim's declared `library` attribute),
//      which mismatches the manifest `library_tag`
//      (`provekit-shim-{serde-json,blake3,stdio}-rust`) the RPC lift filters on.
//   3. Those raw tags are not valid realize routing tags anyway (`materialize`
//      refuses `serde_json` / `std::io` with `expected [a-z][a-z0-9-]*`), and no
//      `.provekit/library-bindings.json` surface routes their concepts.
// Wiring an end-to-end rust consumer (manifest `sugar_proof` + tag alignment)
// is the next enabler; until it lands, the per-shim end-to-end test cannot be
// written without inventing a route that does not exist. The DELETIONS stand on
// their own: nothing live ever reached those orphan disk caches.

use std::path::PathBuf;

use provekit_proof_envelope::cbor_decode;
use serde_json::Value as Json;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("provekit-cli has rust workspace parent")
        .parent()
        .expect("rust workspace has implementations parent")
        .parent()
        .expect("implementations dir has repo parent")
        .to_path_buf()
}

/// The four canonical-bodies disk caches deleted in the fan-out commit. Re-adding
/// any of them would re-introduce a stale projection of the shim `.proof` (the
/// `serde_json` one literally carried `concept:rfc8785-jcs-encode`, a different
/// concept served by the live rfc8785-jcs shim) and could let the realize kit
/// silently disk-load instead of taking the `.proof`-over-RPC authority path.
const DELETED_ORPHAN_CACHES: &[&str] = &[
    "rust-canonical-bodies-serde_json.json",
    "rust-canonical-bodies-blake3.json",
    "rust-canonical-bodies-std::io.json",
    "rust-canonical-bodies-libprovekit-rpc-cross-platform.json",
];

fn assert_orphan_disk_caches_gone() {
    let body_dir = repo_root()
        .join("menagerie")
        .join("rust-language-signature")
        .join("specs")
        .join("body-templates");
    for name in DELETED_ORPHAN_CACHES {
        let cache = body_dir.join(name);
        assert!(
            !cache.exists(),
            "orphan rust canonical-bodies cache {} is back on disk. It was deleted because \
             its bodies are supplied by the shim `.proof` (the `.proof`-load-via-RPC migration). \
             Re-introducing it risks the realize kit disk-loading a stale projection instead of \
             taking the shim `.proof` over RPC as the authority.",
            cache.display()
        );
    }
}

/// Decode every `library-sugar-binding-entry` for `target_language == "rust"`
/// out of the shim's `.proof`, using the SAME path `cmd_materialize` uses to
/// lift body templates (`cbor_decode::decode` -> `members` map -> per-member
/// bstr -> utf8 -> serde_json -> `body.kind`). Returns
/// (concept_name, body_text) pairs. A match here is exactly what the realize
/// kit would receive over RPC once the manifest is wired, so it proves the
/// bodies the deleted disk JSON used to hold are present at the authority.
fn rust_binding_bodies_from_shim_proof(shim_dir_name: &str) -> Vec<(String, String)> {
    let shim_dir = repo_root().join("examples").join(shim_dir_name);
    let proof = std::fs::read_dir(&shim_dir)
        .unwrap_or_else(|e| panic!("read shim dir {}: {e}", shim_dir.display()))
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".proof"))
        })
        .unwrap_or_else(|| panic!("shim {shim_dir_name} has no .proof envelope"));

    let bytes = std::fs::read(&proof).expect("read shim .proof");
    let catalog = cbor_decode::decode(&bytes).expect("decode shim .proof catalog (CBOR)");
    let root = catalog.as_map().expect("shim .proof catalog is a map");
    let members = root
        .get("members")
        .and_then(cbor_decode::CborValue::as_map)
        .expect("shim .proof catalog has a `members` map");

    let mut out = Vec::new();
    for member in members.values() {
        let Some(member_bytes) = member.as_bstr() else {
            continue;
        };
        let Ok(member_text) = std::str::from_utf8(member_bytes) else {
            continue;
        };
        let Ok(member_json) = serde_json::from_str::<Json>(member_text) else {
            continue;
        };
        let Some(body) = member_json.get("body") else {
            continue;
        };
        if body.get("kind").and_then(Json::as_str) != Some("library-sugar-binding-entry") {
            continue;
        }
        if body.get("target_language").and_then(Json::as_str) != Some("rust") {
            continue;
        }
        let Some(concept) = body.get("concept_name").and_then(Json::as_str) else {
            continue;
        };
        let body_text = body
            .get("body_source")
            .and_then(|s| s.get("body_text"))
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string();
        out.push((concept.to_string(), body_text));
    }
    out
}

/// Assert the shim `.proof` carries each `(concept, body_fragment)` and that the
/// orphan disk caches are gone — so a green body assert here is itself proof the
/// bodies come from the shim `.proof` (the authority), not the deleted disk JSON.
fn assert_shim_proof_supplies(
    shim_dir_name: &str,
    expected: &[(&str, &str)],
) {
    assert_orphan_disk_caches_gone();
    let bodies = rust_binding_bodies_from_shim_proof(shim_dir_name);
    for (concept, fragment) in expected {
        let found = bodies
            .iter()
            .find(|(c, _)| c == concept)
            .unwrap_or_else(|| {
                panic!(
                    "shim {shim_dir_name} `.proof` is missing a rust binding for {concept}; \
                     got concepts {:?}",
                    bodies.iter().map(|(c, _)| c.as_str()).collect::<Vec<_>>()
                )
            });
        assert!(
            found.1.contains(fragment),
            "shim {shim_dir_name} `.proof` body for {concept} must contain `{fragment}` \
             (the realize kit splices this body over RPC). Got:\n{}",
            found.1
        );
    }
}

#[test]
fn serde_json_rust_shim_proof_supplies_json_bodies() {
    // Replaces the deleted `rust-canonical-bodies-serde_json.json` (which, note,
    // actually carried the misfiled `concept:rfc8785-jcs-encode`, not these).
    assert_shim_proof_supplies(
        "provekit-shim-serde-json-rust",
        &[
            ("concept:json-parse", "serde_json::from_str(s)"),
            ("concept:json-serialize", "serde_json::to_string(v)"),
        ],
    );
}

#[test]
fn blake3_rust_shim_proof_supplies_hash_bodies() {
    // Replaces the deleted `rust-canonical-bodies-blake3.json`. (The surviving
    // `rust-canonical-bodies-provekit-shim-blake3-rust.json` is a DIFFERENT,
    // manifest-tag-keyed file carrying `concept:hex-encode-lowercase` — no
    // overlap with these blake3-hash concepts.)
    assert_shim_proof_supplies(
        "provekit-shim-blake3-rust",
        &[
            ("concept:blake3-512-of", "blake3::Hasher::new()"),
            ("concept:blake3-hasher-new", "Hasher::new()"),
            ("concept:blake3-hasher-update", "hasher.update(bytes)"),
            (
                "concept:blake3-hasher-finalize-xof-64",
                "finalize_xof().fill(&mut out)",
            ),
        ],
    );
}

#[test]
fn stdio_rust_shim_proof_supplies_io_bodies() {
    // Replaces the deleted `rust-canonical-bodies-std::io.json`.
    assert_shim_proof_supplies(
        "provekit-shim-stdio-rust",
        &[
            ("concept:stdio-read-line", "io::stdin()"),
            ("concept:stdio-write-line", "io::stdout()"),
            ("concept:stderr-write-line", "io::stderr()"),
        ],
    );
}

// NOTE on the 4th deleted JSON (`rust-canonical-bodies-libprovekit-rpc-cross-
// platform.json`): there is NO `provekit-shim-libprovekit-rpc-cross-platform`
// shim and NO `.proof` to source bodies from, so there is nothing to
// regression-test here. That tag appears in
// `provekit-realize-rust-core/src/lib.rs` ONLY as a literal demo/library label
// inside test-fixture `#[provekit::sugar(...)]` source strings (the body comes
// from the resolved AST, not a bodies JSON), and no `library-bindings.json`
// surface or realize manifest routes to it. Its deletion stands on that
// orphan-routing evidence alone.
#[test]
fn libprovekit_rpc_cross_platform_orphan_cache_stays_deleted() {
    let cache = repo_root()
        .join("menagerie")
        .join("rust-language-signature")
        .join("specs")
        .join("body-templates")
        .join("rust-canonical-bodies-libprovekit-rpc-cross-platform.json");
    assert!(
        !cache.exists(),
        "the `libprovekit-rpc-cross-platform` canonical-bodies cache was a pure orphan \
         (no shim, no manifest, no bindings-surface route) and must stay deleted: {}",
        cache.display()
    );
}
