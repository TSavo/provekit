// SPDX-License-Identifier: Apache-2.0
//
// baseline_pin: regression test pinning the Rust std baseline v1
// catalog's contractSetCid + disclaimer_cid.
//
// Either of these CIDs changing means the catalog body changed
// (predicates, builtins, disclaimer text, foundation seed). That's a
// signing event, not a Makefile string edit: bump v1 to v1.1 (minor
// extension) or v2 (major), and update the pinned values below.
//
// Per the rubric §"Versioning":
//   * Adding builtins without semantic changes -> v1.<minor>
//   * Major language version change (rustc 1.x -> 2.x) -> v2 catalog
//
// The bundle CID intentionally is NOT pinned here: it covers the
// signed envelope metadata + signer pubkey + signature, which is
// orthogonal to "the contracts are the same." The contractSetCid IS
// the trust anchor: same contracts, same set CID across signers,
// across kits, across rebuilds.

use provekit_baseline_rust_std::{compute_disclaimer_cid, disclaimer_text, mint_baseline};

const PINNED_CONTRACT_SET_CID: &str = "blake3-512:76c278afe2f60f5b58ebaf53df1078143204cddbbf47d23119fb9778e17b6488004b3f8d8ca471720c1774483ea0fc7cb6dba0a097c638cc7e8231edb566d5e4";
const PINNED_DISCLAIMER_CID: &str = "blake3-512:dae426aaf69da3fb28d1b45bc61dd700e3b3e1f0637814d24ec9071071d2fe9f32befb5b4211cd4ae8d6596e67e7a12eecff849ffdbf8a832b376fc7db19bca1";

fn tempdir() -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);

    let mut p = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    p.push(format!(
        "provekit-rust-std-baseline-pin-{nanos}-{}-{seq}",
        std::process::id()
    ));
    p
}

#[test]
fn contract_set_cid_is_pinned() {
    let dir = tempdir();
    let m = mint_baseline(&dir).expect("mint");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        m.contract_set_cid, PINNED_CONTRACT_SET_CID,
        "contractSetCid drift: catalog body changed without a version bump.\n\
         Got:      {}\n\
         Pinned:   {}\n\
         If this change is intentional, update PINNED_CONTRACT_SET_CID and \
         either bump the Rust std baseline to v1.<minor> (extension) or \
         v2 (semantic break).",
        m.contract_set_cid, PINNED_CONTRACT_SET_CID
    );
}

#[test]
fn disclaimer_cid_is_pinned() {
    let cid = compute_disclaimer_cid(&disclaimer_text());
    assert_eq!(
        cid, PINNED_DISCLAIMER_CID,
        "disclaimer_cid drift: disclaimer base or addendum text changed.\n\
         Got:      {}\n\
         Pinned:   {}\n\
         The base text is locked verbatim per baseline-catalog-rubric.md §4.\n\
         If the addendum legitimately changed, update PINNED_DISCLAIMER_CID \
         and re-mint.",
        cid, PINNED_DISCLAIMER_CID
    );
}

/// Substrate verification: the verifier's `load_all_proofs` re-derives
/// every member's CID from its bytes (rule 2) and rejects mismatches.
/// Running it against the freshly-minted catalog asserts every member
///: including the `kind=disclaimer` envelope: round-trips cleanly.
///
/// This is the issue #257 AC clause "at minimum the substrate verifies
/// the bytes": instead of wiring a new `provekit verify --baseline=rust`
/// CLI verb, we lean on the existing `Runner::run_load_and_enumerate`
/// path that every `.proof`-aware tool funnels through.
#[test]
fn substrate_loads_baseline_with_no_errors() {
    use provekit_verifier::{Runner, RunnerConfig};

    let dir = tempdir();
    let _ = mint_baseline(&dir).expect("mint");

    let cfg = RunnerConfig {
        project_root: dir.clone(),
        ..Default::default()
    };
    let runner = Runner::new(cfg);
    let (pool, _callsites) = runner.run_load_and_enumerate();
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        pool.load_errors.is_empty(),
        "verifier rejected baseline members: {:#?}",
        pool.load_errors
    );
    // The pool should hold all 158 mementos (157 contracts + 1 disclaimer).
    assert!(
        pool.mementos.len() >= 100,
        "verifier indexed only {} mementos; expected >= 100",
        pool.mementos.len()
    );
}

#[test]
fn builtin_count_at_or_above_floor() {
    let dir = tempdir();
    let m = mint_baseline(&dir).expect("mint");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        m.distinct_builtin_count >= 50,
        "rubric floor: builtin count {} < 50",
        m.distinct_builtin_count
    );
    assert!(
        m.contract_count >= 100,
        "rubric floor: contract count {} < 100 (50 builtins * 2 predicates)",
        m.contract_count
    );
}
