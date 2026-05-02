// SPDX-License-Identifier: Apache-2.0
//
// .proof catalog envelope tests. Pins:
//   - filename CID matches BLAKE3-512 of the catalog bytes (trust-root)
//   - CID is "blake3-512:" + 128 hex chars
//   - Same input -> same bytes (deterministic across runs)
//   - Map head reflects 7 keys: kind, name, version, members, signer,
//     declaredAt, signature
//   - members map keys are CBOR-text-string-encoded and contain member
//     bytes as a CBOR byte string

use std::collections::BTreeMap;

use provekit_canonicalizer::blake3_512_of;
use provekit_proof_envelope::{build_proof_envelope, ProofEnvelopeInput};

fn fixture_input() -> ProofEnvelopeInput {
    let mut members = BTreeMap::new();
    members.insert(
        "blake3-512:aa".to_string(),
        b"{\"hello\":\"world\"}".to_vec(),
    );
    members.insert(
        "blake3-512:bb".to_string(),
        b"{\"goodbye\":\"world\"}".to_vec(),
    );
    ProofEnvelopeInput {
        name: "@test/cat".to_string(),
        version: "1.0.0".to_string(),
        binary_cid: None,
        members,
        signer_cid: "blake3-512:cc".to_string(),
        signer_seed: [0x42u8; 32],
        declared_at: "2026-04-30T00:00:00.000Z".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Trust root: filename CID == BLAKE3-512 of bytes
// ---------------------------------------------------------------------------

#[test]
fn cid_equals_blake3_512_of_emitted_bytes() {
    let input = fixture_input();
    let out = build_proof_envelope(&input);
    let recomputed = blake3_512_of(&out.bytes);
    assert_eq!(out.cid, recomputed);
}

#[test]
fn cid_has_blake3_512_prefix() {
    let input = fixture_input();
    let out = build_proof_envelope(&input);
    assert!(out.cid.starts_with("blake3-512:"));
}

#[test]
fn cid_total_length_is_prefix_plus_128() {
    let input = fixture_input();
    let out = build_proof_envelope(&input);
    assert_eq!(out.cid.len(), "blake3-512:".len() + 128);
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

#[test]
fn same_input_produces_identical_bytes() {
    let a = build_proof_envelope(&fixture_input());
    let b = build_proof_envelope(&fixture_input());
    assert_eq!(a.bytes, b.bytes);
    assert_eq!(a.cid, b.cid);
}

#[test]
fn member_insertion_order_does_not_matter() {
    // Members go in via BTreeMap (already sorted). Building two
    // ProofEnvelopeInputs with members inserted in different orders
    // must yield identical bytes (BTreeMap normalizes order).
    let mut m1 = BTreeMap::new();
    m1.insert("blake3-512:bb".to_string(), b"second".to_vec());
    m1.insert("blake3-512:aa".to_string(), b"first".to_vec());

    let mut m2 = BTreeMap::new();
    m2.insert("blake3-512:aa".to_string(), b"first".to_vec());
    m2.insert("blake3-512:bb".to_string(), b"second".to_vec());

    let mk = |members: BTreeMap<String, Vec<u8>>| ProofEnvelopeInput {
        name: "x".into(),
        version: "1".into(),
        binary_cid: None,
        members,
        signer_cid: "blake3-512:cc".into(),
        signer_seed: [0u8; 32],
        declared_at: "2026-04-30T00:00:00.000Z".into(),
    };
    assert_eq!(
        build_proof_envelope(&mk(m1)).bytes,
        build_proof_envelope(&mk(m2)).bytes
    );
}

// ---------------------------------------------------------------------------
// Catalog map shape
// ---------------------------------------------------------------------------

#[test]
fn signed_catalog_map_head_is_seven_keys() {
    let input = fixture_input();
    let out = build_proof_envelope(&input);
    // 7-key map head: major 5 (0xA0) + count 7 = 0xA7.
    assert_eq!(out.bytes[0], 0xA7);
}

#[test]
fn empty_members_still_produces_valid_envelope() {
    let input = ProofEnvelopeInput {
        name: "x".into(),
        version: "1".into(),
        binary_cid: None,
        members: BTreeMap::new(),
        signer_cid: "blake3-512:cc".into(),
        signer_seed: [0u8; 32],
        declared_at: "2026-04-30T00:00:00.000Z".into(),
    };
    let out = build_proof_envelope(&input);
    assert_eq!(out.bytes[0], 0xA7);
    assert!(out.cid.starts_with("blake3-512:"));
}

// ---------------------------------------------------------------------------
// Sensitivity: changing any input field changes the CID
// ---------------------------------------------------------------------------

#[test]
fn changing_name_changes_cid() {
    let mut a = fixture_input();
    let mut b = fixture_input();
    b.name = "@other/name".into();
    assert_ne!(build_proof_envelope(&a).cid, build_proof_envelope(&b).cid);
    a.name = "@test/cat".into(); // sanity
    let _ = a;
}

#[test]
fn changing_version_changes_cid() {
    let mut b = fixture_input();
    b.version = "2.0.0".into();
    assert_ne!(
        build_proof_envelope(&fixture_input()).cid,
        build_proof_envelope(&b).cid
    );
}

#[test]
fn changing_members_changes_cid() {
    let mut b = fixture_input();
    b.members
        .insert("blake3-512:dd".into(), b"different".to_vec());
    assert_ne!(
        build_proof_envelope(&fixture_input()).cid,
        build_proof_envelope(&b).cid
    );
}

#[test]
fn changing_signer_cid_changes_cid() {
    let mut b = fixture_input();
    b.signer_cid = "blake3-512:other".into();
    assert_ne!(
        build_proof_envelope(&fixture_input()).cid,
        build_proof_envelope(&b).cid
    );
}

#[test]
fn changing_signer_seed_changes_cid() {
    let mut b = fixture_input();
    b.signer_seed = [0x99u8; 32];
    // The signature bytes change with a new seed, so the catalog bytes
    // (and hence CID) change too.
    assert_ne!(
        build_proof_envelope(&fixture_input()).cid,
        build_proof_envelope(&b).cid
    );
}

#[test]
fn changing_declared_at_changes_cid() {
    let mut b = fixture_input();
    b.declared_at = "2099-12-31T23:59:59.999Z".into();
    assert_ne!(
        build_proof_envelope(&fixture_input()).cid,
        build_proof_envelope(&b).cid
    );
}

// ---------------------------------------------------------------------------
// Member-bytes filename CID rule (independent of catalog wrapping)
// ---------------------------------------------------------------------------

#[test]
fn catalog_member_filename_rule_matches_blake3_of_value_bytes() {
    // The .proof grammar says: a member whose CID is in the catalog map
    // has, as its CBOR value, a byte string equal to that member's
    // canonical bytes. We don't decode the CBOR here; we check the
    // implication: rebuilding with a different value for the same CID
    // must change the catalog CID.
    let mut a_members = BTreeMap::new();
    a_members.insert("blake3-512:aa".into(), b"original".to_vec());
    let mut b_members = BTreeMap::new();
    b_members.insert("blake3-512:aa".into(), b"tampered".to_vec());

    let mk = |members: BTreeMap<String, Vec<u8>>| ProofEnvelopeInput {
        name: "x".into(),
        version: "1".into(),
        binary_cid: None,
        members,
        signer_cid: "blake3-512:cc".into(),
        signer_seed: [0u8; 32],
        declared_at: "2026-04-30T00:00:00.000Z".into(),
    };
    assert_ne!(
        build_proof_envelope(&mk(a_members)).cid,
        build_proof_envelope(&mk(b_members)).cid
    );
}
