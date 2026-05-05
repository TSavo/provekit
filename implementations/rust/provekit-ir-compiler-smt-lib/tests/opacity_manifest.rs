// SPDX-License-Identifier: Apache-2.0
//
// Pinned fixture for the OpacityManifest emission added in #332.
//
// The SMT-LIB compiler cannot soundly translate `Sort::Function`
// (predicate quantification) or `Sort::Dependent` (value-dependent
// types). For each parent IR node carrying such a sort, the compiler
// emits an OpacityManifest entry with a content-addressed positionCid
// and a reason code from the closed enum in
// `protocol/specs/2026-05-02-opacity-manifest-grammar.md` §4.
//
// This file pins the byte form of a representative manifest. Drift in
// the JCS encoder, the BLAKE3 helper, the field naming, or the entry
// sort order would surface here, even when other tests still pass.

use serde_json::json;

use provekit_ir_compiler_smt_lib::{
    compile_to_parts_with_manifest, OpacityManifest, REASON_DEPENDENT_TYPE,
    REASON_PREDICATE_QUANTIFICATION,
};

// ---------------------------------------------------------------------------
// Fixture: a Forall over a function sort + an Exists over a dependent sort
// ---------------------------------------------------------------------------
//
// Both quantifiers reduce to the same parent-opaque shape under §4 of
// the spec. We use one of each so the manifest exercises both reason
// codes in a single round-trip.
//
// We do NOT bury these inside an outer `And`; placing them at the top
// keeps the fixture's positionCid stable against future emit-side
// refactors that might rewrap the formula.

fn fixture_function_sort_quantifier() -> serde_json::Value {
    json!({
        "kind": "and",
        "operands": [
            {
                "kind": "forall",
                "name": "P",
                "sort": {
                    "kind": "function",
                    "args": [{"kind": "primitive", "name": "Int"}],
                    "return": {"kind": "primitive", "name": "Bool"}
                },
                "body": {
                    "kind": "atomic", "name": "=",
                    "args": [
                        {"kind": "var", "name": "x"},
                        {"kind": "const", "value": 0,
                         "sort": {"kind": "primitive", "name": "Int"}}
                    ]
                }
            },
            {
                "kind": "exists",
                "name": "v",
                "sort": {
                    "kind": "dependent",
                    "name": "Vec",
                    "indexVar": "n",
                    "indexSort": {"kind": "primitive", "name": "Int"}
                },
                "body": {
                    "kind": "atomic", "name": "=",
                    "args": [
                        {"kind": "var", "name": "y"},
                        {"kind": "const", "value": 0,
                         "sort": {"kind": "primitive", "name": "Int"}}
                    ]
                }
            }
        ]
    })
}

#[test]
fn manifest_for_function_sort_marks_predicate_quantification() {
    let ir = fixture_function_sort_quantifier();
    let (_compiled, manifest) = compile_to_parts_with_manifest(&ir).expect("compile");
    assert_eq!(manifest.opacities.len(), 2);
    let codes: Vec<&str> = manifest
        .opacities
        .iter()
        .map(|e| e.reason_code.as_str())
        .collect();
    assert!(codes.contains(&REASON_PREDICATE_QUANTIFICATION));
    assert!(codes.contains(&REASON_DEPENDENT_TYPE));
}

#[test]
fn manifest_entries_sorted_by_position_cid_ascending() {
    let ir = fixture_function_sort_quantifier();
    let (_, manifest) = compile_to_parts_with_manifest(&ir).expect("compile");
    let mut sorted = manifest.opacities.clone();
    sorted.sort_by(|a, b| {
        a.position_cid
            .cmp(&b.position_cid)
            .then_with(|| a.reason_code.cmp(&b.reason_code))
    });
    assert_eq!(manifest.opacities, sorted, "opacities must be pre-sorted");
}

#[test]
fn manifest_top_level_field_set_matches_spec_grammar() {
    let m = OpacityManifest::empty();
    let bytes = m.to_canonical_bytes();
    let s = std::str::from_utf8(&bytes).expect("utf8");
    // JCS sorts keys lexicographically: compiler, compilerVersion,
    // opacities, protocolVersion. Spec §2 grammar requires exactly
    // these four top-level fields.
    assert_eq!(
        s,
        r#"{"compiler":"smt-lib-reference","compilerVersion":"0.1.0","opacities":[],"protocolVersion":"ir-compiler-protocol/2"}"#
    );
}

// ---------------------------------------------------------------------------
// Pinned fixture: byte-equality against frozen bytes
// ---------------------------------------------------------------------------
//
// The fixture below is the canonical JCS bytes of the manifest emitted
// by the in-process SMT-LIB compiler when handed the formula in
// `fixture_function_sort_quantifier()` above. Regenerating these
// bytes is intentional friction: any change to the manifest emission
// (field order, sort order, reason-code spelling, position-CID hash
// algorithm, JCS encoder) requires updating the constant, which makes
// the change visible in code review.
//
// Procedure to regenerate when the spec or compiler version genuinely
// shifts:
//
//   1. Run `cargo test -p provekit-ir-compiler-smt-lib opacity_manifest \
//        -- --nocapture` and copy the printed bytes from the
//      `manifest_canonical_bytes_match_pinned_fixture` test.
//   2. Replace `EXPECTED_MANIFEST_BYTES` below with the new bytes.
//   3. Update the catalog if the OpacityManifest grammar's spec CID
//      changed; otherwise this is a compiler-version bump only.
//
// The first 16 bytes of the manifest are also asserted independently
// so reviewers can sanity-check determinism without diffing the full
// fixture.

const EXPECTED_MANIFEST_BYTES: &[u8] = b"{\"compiler\":\"smt-lib-reference\",\"compilerVersion\":\"0.1.0\",\"opacities\":[{\"positionCid\":\"blake3-512:3bfae90a939c9e485c26548fe8b88b7c5ee80b95854e9919064c8a1ddbd6803072fea7ec842138e8f9546994cf44db665787ac52ee4872c5b302666787df63f5\",\"reasonCode\":\"dependent_type\"},{\"positionCid\":\"blake3-512:4a74356b72172a74bdcbcccb1e644de6d0051730ca46fd7abdc7265e7cbddcc2545ee379dc52ee0c75eefb928b205e15f7327f58988a35c959545add1fc429c2\",\"reasonCode\":\"predicate_quantification\"}],\"protocolVersion\":\"ir-compiler-protocol/2\"}";

#[test]
fn manifest_canonical_bytes_match_pinned_fixture() {
    let ir = fixture_function_sort_quantifier();
    let (_compiled, manifest) =
        compile_to_parts_with_manifest(&ir).expect("compile");
    let actual = manifest.to_canonical_bytes();

    // Print first-16 prefix for human sanity-check (visible in
    // `cargo test -- --nocapture` output).
    let prefix: Vec<String> =
        actual.iter().take(16).map(|b| format!("{:02x}", b)).collect();
    eprintln!("manifest first-16 bytes: {}", prefix.join(" "));

    if actual != EXPECTED_MANIFEST_BYTES {
        let actual_str =
            std::str::from_utf8(&actual).unwrap_or("<non-utf8>");
        panic!(
            "manifest bytes drifted from pinned fixture.\n\
             EXPECTED ({} bytes):\n{}\n\
             ACTUAL ({} bytes):\n{}",
            EXPECTED_MANIFEST_BYTES.len(),
            std::str::from_utf8(EXPECTED_MANIFEST_BYTES)
                .unwrap_or("<non-utf8>"),
            actual.len(),
            actual_str
        );
    }
}

#[test]
fn manifest_first_16_bytes_match_pinned_prefix() {
    let ir = fixture_function_sort_quantifier();
    let (_compiled, manifest) =
        compile_to_parts_with_manifest(&ir).expect("compile");
    let actual = manifest.to_canonical_bytes();
    // First 16 bytes start the JCS object: `{"compiler":"sm`
    assert_eq!(&actual[..16], &EXPECTED_MANIFEST_BYTES[..16]);
}

#[test]
fn manifest_byte_output_is_deterministic_across_runs() {
    let ir = fixture_function_sort_quantifier();
    let (_, m1) = compile_to_parts_with_manifest(&ir).expect("compile");
    let (_, m2) = compile_to_parts_with_manifest(&ir).expect("compile");
    assert_eq!(m1.to_canonical_bytes(), m2.to_canonical_bytes());
}
