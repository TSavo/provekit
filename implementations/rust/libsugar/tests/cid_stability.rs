// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

use libsugar::canonical::{json_jcs, serializable_jcs};
use libsugar::proofir_bridge::{BridgeError, CatalogIndex, ResolvedTerm};
use libsugar::{proofir_resolve, proofir_unresolve};
use sugar_ir_types::{Sort, Term};
use serde_json::json;

#[derive(Debug)]
struct FixtureResult {
    path: PathBuf,
    original_canonical_bytes: Vec<u8>,
    unresolved_canonical_bytes: Vec<u8>,
    resolved_cid: String,
}

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

fn proofir_fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("proofir")
}

fn proofir_fixtures() -> Vec<PathBuf> {
    let fixture_dir = proofir_fixture_dir();
    let mut fixtures = Vec::new();
    collect_proofir_json(&fixture_dir, &mut fixtures);
    fixtures.sort();
    fixtures
}

fn collect_proofir_json(dir: &Path, fixtures: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries {
        let entry = entry.expect("read ProofIR fixture directory entry");
        let path = entry.path();
        if path.is_dir() {
            collect_proofir_json(&path, fixtures);
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".proofir.json"))
        {
            fixtures.push(path);
        }
    }
}

fn concept_catalog() -> CatalogIndex {
    CatalogIndex::from_catalog_root(repo_root().join("menagerie/concept-shapes/catalog"))
        .expect("concept-shapes catalog loads")
}

fn fixture_round_trip(path: &Path, catalog: &CatalogIndex) -> FixtureResult {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("read ProofIR fixture {}: {error}", path.display()));
    let original_json = serde_json::from_str::<serde_json::Value>(&text)
        .unwrap_or_else(|error| panic!("parse ProofIR JSON {}: {error}", path.display()));
    let original = serde_json::from_value::<Term>(original_json.clone())
        .unwrap_or_else(|error| panic!("decode ProofIR term {}: {error}", path.display()));

    let resolved = proofir_resolve(&original, catalog)
        .unwrap_or_else(|error| panic!("resolve ProofIR fixture {}: {error}", path.display()));
    let resolved_cid = resolved_cid(&resolved);
    let unresolved = proofir_unresolve(&resolved, catalog)
        .unwrap_or_else(|error| panic!("unresolve ProofIR fixture {}: {error}", path.display()));
    let unresolved_json = serde_json::to_value(&unresolved)
        .unwrap_or_else(|error| panic!("encode unresolved ProofIR {}: {error}", path.display()));

    FixtureResult {
        path: path.to_path_buf(),
        original_canonical_bytes: json_jcs(&original_json)
            .unwrap_or_else(|error| {
                panic!("canonicalize original ProofIR {}: {error}", path.display())
            })
            .into_bytes(),
        unresolved_canonical_bytes: json_jcs(&unresolved_json)
            .unwrap_or_else(|error| {
                panic!(
                    "canonicalize unresolved ProofIR {}: {error}",
                    path.display()
                )
            })
            .into_bytes(),
        resolved_cid,
    }
}

fn resolved_cid(resolved: &ResolvedTerm) -> String {
    let resolved_jcs = serializable_jcs(resolved).expect("resolved term JCS");
    sugar_canonicalizer::blake3_512_of(resolved_jcs.as_bytes())
}

#[test]
fn proofir_resolve_unresolve_is_cid_stable_for_committed_fixtures() {
    let fixtures = proofir_fixtures();
    assert!(
        !fixtures.is_empty(),
        "no ProofIR term fixtures found under {}",
        proofir_fixture_dir().display()
    );

    let catalog = concept_catalog();
    let mut mismatches = Vec::new();
    let mut cids = Vec::new();

    for fixture in fixtures {
        let result = fixture_round_trip(&fixture, &catalog);
        cids.push((result.path.clone(), result.resolved_cid));
        if result.original_canonical_bytes != result.unresolved_canonical_bytes {
            mismatches.push(result.path);
        }
    }

    assert!(
        mismatches.is_empty(),
        "cid stability: {} fixtures, {} mismatches: {:?}; resolved CIDs: {:?}",
        cids.len(),
        mismatches.len(),
        mismatches,
        cids
    );
}

#[test]
fn broken_fixture_refuses_with_typed_bridge_failure() {
    let catalog = concept_catalog();
    let broken = Term::Ctor {
        name: "concept:add".to_string(),
        args: vec![Term::Const {
            value: json!(1),
            sort: Sort::Primitive {
                name: "Int".to_string(),
            },
        }],
    };

    let error = proofir_resolve(&broken, &catalog).expect_err("broken fixture refuses");

    assert_eq!(
        error,
        BridgeError::ArityMismatch {
            expected: 3,
            actual: 1
        }
    );
}
