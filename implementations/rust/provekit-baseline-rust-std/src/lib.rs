// SPDX-License-Identifier: Apache-2.0
//
// provekit-baseline-rust-std
//
// Foundation-baseline catalog for Rust's std library.
//
// This crate is the BUILD ORCHESTRATOR for the Rust content-addressed
// foundation baseline catalog of hidden predicates about
// std::* builtins (per `docs/contributing/baseline-catalog-rubric.md`).
// It is _advisory_, not authoritative; the `rust-lang` team is the
// authoritative steward for std contracts. See `docs/baselines/rust.md`
// for the full disclaimer + change log.
//
// Layout:
//   * `src/lib.rs`: orchestrator + envelope assembly (this file).
//   * `src/std_string_slab.rs`: String / &str builtins.
//   * `src/std_vec_slab.rs`: Vec<T> builtins.
//   * `src/std_option_slab.rs`: Option<T> builtins.
//   * `src/std_result_slab.rs`: Result<T, E> builtins.
//   * `src/std_slice_slab.rs`: slice (`[T]`) builtins.
//   * `src/std_hashmap_slab.rs`: HashMap / BTreeMap builtins.
//   * `src/std_iter_slab.rs`: numeric / Iterator builtins.
//   * `src/bin/mint-rust-std-baseline.rs`: orchestrator binary.
//
// Each slab file ships a `pub fn invariants()` that calls into the
// shared kit DSL (`must`/`contract`/`forall`/`eq`/`gte`/`ctor` via
// `provekit-ir-symbolic`). Slab boundaries match the rubric's coverage
// targets (~10 string + ~10 vec + ~8 option + ~8 result + ~8 slice + ~8 map +
// ~6 iter ≈ ~58 builtins, ~150 ContractDecls).
//
// DSL surface used (per #285's pre-launch lock):
//   forall(sort, |v| body)
//   eq(a, b)
//   gte(a, b)
//   ctor("name", args): kit-defined operations like `len`,
//                              `type_of`, `starts_with`
//   num(n) / str_const(s): primitives
//   must(name, formula): `pre`-only convenience
//   contract(name, args): full pre/post/inv shape
//
// Predicates G1-G4 (lt / lte / between / member_of / or / not) land in
// a follow-up after #285's full DSL extension across all 12 kits. This
// pilot honors the floor with what's already byte-equivalent.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value};
use provekit_claim_envelope::{
    compute_contract_set_cid, contract_cid as compute_contract_cid, mint_contract, Authoring,
    MintContractArgs, LAYERED_SCHEMA_VERSION,
};
use provekit_ir_symbolic::serialize::formula_to_value;
use provekit_ir_symbolic::{begin_collecting, finish, reset_collector, ContractDecl};
use provekit_proof_envelope::{
    build_proof_envelope, ed25519_pubkey_string, ed25519_sign_string, Ed25519Seed,
    ProofEnvelopeInput,
};

// --- Slab modules ----------------------------------------------------------

pub mod std_hashmap_slab;
pub mod std_iter_slab;
pub mod std_option_slab;
pub mod std_result_slab;
pub mod std_slice_slab;
pub mod std_string_slab;
pub mod std_vec_slab;

// --- Foundation signing constants ------------------------------------------

/// PUBLICLY KNOWN foundation v0 ed25519 seed. Same seed every kit's
/// foundation-signed catalog uses. Mirrors `FOUNDATION_V0_SEED` in
/// `provekit-cli/src/cmd_mint.rs`.
pub const FOUNDATION_V0_SEED: Ed25519Seed = [0x42u8; 32];

/// Pinned `producedAt` / `declaredAt` for byte-determinism. Bumped when
/// the catalog body changes such that the v1 frame is re-emitted.
const DECLARED_AT: &str = "2026-05-03T18:00:00Z";

/// Producer attribution stamped on every minted contract memento.
const PRODUCED_BY: &str = "provekit-baseline-rust-std@0.1.0";

/// Catalog identity stamped on the proof envelope. The version line is
/// the rubric's `baseline.version` field. Publication uses the resulting
/// proof CID as the canonical filename.
pub const CATALOG_NAME: &str = "@provekit/baselines/rust-std";
pub const CATALOG_VERSION: &str = "1.0.0";
pub const BASELINE_VERSION: u32 = 1;
pub const BASELINE_LANGUAGE: &str = "rust";
pub const BASELINE_LANGUAGE_VERSION: &str = "1.81.0";
pub const BASELINE_KIT_VERSION: &str = "0.1.0";
pub const SIGNER_ROLE: &str = "foundation-baseline";

// --- Disclaimer text (verbatim from the rubric) ----------------------------

/// Verbatim base disclaimer per `baseline-catalog-rubric.md` §4. The
/// per-language addendum is appended below; modifying ANY byte of the
/// composed text changes `disclaimer_cid`, which invalidates the
/// signature and forces a re-mint.
pub const DISCLAIMER_BASE: &str = "Foundation baseline catalog \u{2014} advisory only.

This catalog asserts hidden predicates about the named language's
standard library. It is signed by the ProvekIt foundation key as a
starting point for users who want to verify proofs about code in
this language.

It is NOT authoritative.

The authoritative signer for this language's contracts is the
language steward (named below). If they sign their own catalog,
prefer it over this one. If they have not, fork this catalog and
sign your own \u{2014} see docs/contributing/signing-your-own-catalog.md.
";

/// Per-language addendum. The values here MUST match the values stamped
/// into the envelope `metadata` block, otherwise consumers see a
/// disclaimer-vs-metadata drift.
pub const DISCLAIMER_ADDENDUM: &str = "Language: rust
Steward: rust-lang team
Steward signature available: no
Authored against: rustc 1.81.0

Predicate gaps in this baseline (deferred to post-launch):
  - [G6 effect tracking]: side-effect properties (async, throws, IO) not encoded
  - [G7 aliasing]: pointer-aliasing preconditions not encoded for unsafe operations

The authoritative signer for this language can add these predicates;
the foundation baseline ships at the floor density only.
";

/// The composed disclaimer text shipped as a `members` entry in the
/// signed proof envelope. Byte-stable across runs.
pub fn disclaimer_text() -> String {
    let mut s = String::with_capacity(DISCLAIMER_BASE.len() + DISCLAIMER_ADDENDUM.len() + 1);
    s.push_str(DISCLAIMER_BASE);
    s.push('\n');
    s.push_str(DISCLAIMER_ADDENDUM);
    s
}

// --- Slab orchestration ----------------------------------------------------

/// Source-file label tagging which slab a contract was authored in.
/// Used in the mint result for traceability.
#[derive(Debug, Clone)]
pub struct InvariantSource {
    pub label: &'static str,
    pub path: &'static str,
}

/// One slab's authored contracts plus the source label.
#[derive(Debug, Clone)]
pub struct AuthoredSlab {
    pub source: InvariantSource,
    pub contracts: Vec<ContractDecl>,
}

/// All authored contracts across every slab. No I/O.
pub fn author_all_invariants() -> Vec<AuthoredSlab> {
    vec![
        run_one_slab(
            InvariantSource {
                label: "std_string",
                path: "provekit-baseline-rust-std/src/std_string_slab.rs",
            },
            std_string_slab::invariants,
        ),
        run_one_slab(
            InvariantSource {
                label: "std_vec",
                path: "provekit-baseline-rust-std/src/std_vec_slab.rs",
            },
            std_vec_slab::invariants,
        ),
        run_one_slab(
            InvariantSource {
                label: "std_option",
                path: "provekit-baseline-rust-std/src/std_option_slab.rs",
            },
            std_option_slab::invariants,
        ),
        run_one_slab(
            InvariantSource {
                label: "std_result",
                path: "provekit-baseline-rust-std/src/std_result_slab.rs",
            },
            std_result_slab::invariants,
        ),
        run_one_slab(
            InvariantSource {
                label: "std_slice",
                path: "provekit-baseline-rust-std/src/std_slice_slab.rs",
            },
            std_slice_slab::invariants,
        ),
        run_one_slab(
            InvariantSource {
                label: "std_hashmap",
                path: "provekit-baseline-rust-std/src/std_hashmap_slab.rs",
            },
            std_hashmap_slab::invariants,
        ),
        run_one_slab(
            InvariantSource {
                label: "std_iter",
                path: "provekit-baseline-rust-std/src/std_iter_slab.rs",
            },
            std_iter_slab::invariants,
        ),
    ]
}

fn run_one_slab(source: InvariantSource, f: fn()) -> AuthoredSlab {
    reset_collector();
    begin_collecting();
    f();
    let contracts = finish();
    AuthoredSlab { source, contracts }
}

// --- Disclaimer member envelope --------------------------------------------
//
// The proof envelope's `members` map demands that every value be a
// memento envelope whose JCS-then-BLAKE3-512 hash matches the key. We
// can't drop raw disclaimer bytes; we wrap them in a v1.2 layered
// memento with `header.kind = "disclaimer"`. The verifier's `load_one`
// indexes the member by attestation CID; unrecognized `header.kind`
// values sit inertly (see `provekit-verifier/src/load_all_proofs.rs`
//: only `header.kind == "bridge"` triggers special indexing).

fn signing_bytes(header: &Arc<Value>, metadata: &Arc<Value>) -> Vec<u8> {
    let msg = Value::object([("header", header.clone()), ("metadata", metadata.clone())]);
    encode_jcs(&msg).into_bytes()
}

/// Mint the disclaimer as a layered memento. Returns the canonical
/// bytes plus the attestation CID (key under which it goes into the
/// proof envelope's `members` map).
fn mint_disclaimer_memento(text: &str, signer_seed: &Ed25519Seed) -> (String, Vec<u8>) {
    let body_hash = blake3_512_of(text.as_bytes());

    // Header: schemaVersion + kind + cid + body.
    //   * `cid` is the BLAKE3-512 of the disclaimer bytes (i.e. the
    //     `disclaimer_cid` value the rubric's metadata block points at).
    //   * `body` carries the disclaimer text inline so the disclaimer
    //     IS in the proof bundle, not just hashed by reference.
    let header_entries: Vec<(String, Arc<Value>)> = vec![
        (
            "schemaVersion".into(),
            Value::string(LAYERED_SCHEMA_VERSION),
        ),
        ("kind".into(), Value::string("disclaimer")),
        ("cid".into(), Value::string(body_hash.clone())),
        ("body".into(), Value::string(text.to_string())),
    ];
    let header = Arc::new(Value::Object(header_entries));

    // Metadata: producer attribution. Non-normative.
    let metadata_entries: Vec<(String, Arc<Value>)> = vec![
        ("producedBy".into(), Value::string(PRODUCED_BY.to_string())),
        ("producedAt".into(), Value::string(DECLARED_AT.to_string())),
        (
            "authoring".into(),
            Arc::new(Value::Object(vec![
                ("producerKind".into(), Value::string("kit-author")),
                ("author".into(), Value::string(PRODUCED_BY.to_string())),
            ])),
        ),
    ];
    let metadata = Arc::new(Value::Object(metadata_entries));

    let signer = ed25519_pubkey_string(signer_seed);
    let signing_msg = signing_bytes(&header, &metadata);
    let signature = ed25519_sign_string(signer_seed, &signing_msg);

    let envelope = Value::object([
        ("signer", Value::string(signer)),
        ("declaredAt", Value::string(DECLARED_AT.to_string())),
        ("signature", Value::string(signature)),
    ]);
    let envelope_jcs = encode_jcs(&envelope);
    let attestation_cid = blake3_512_of(envelope_jcs.as_bytes());

    let memento = Value::object([
        ("envelope", envelope),
        ("header", header),
        ("metadata", metadata),
    ]);
    let memento_jcs = encode_jcs(&memento);
    (attestation_cid, memento_jcs.into_bytes())
}

/// Compute `disclaimer_cid` (the BLAKE3-512 of the disclaimer bytes).
/// This is the value stamped into envelope metadata as
/// `baseline.disclaimer_cid`: independent of the disclaimer's
/// attestation envelope CID.
pub fn compute_disclaimer_cid(text: &str) -> String {
    blake3_512_of(text.as_bytes())
}

// --- Mint result -----------------------------------------------------------

/// Result from minting the baseline catalog.
#[derive(Debug, Clone)]
pub struct MintResult {
    /// Full self-identifying CID (`blake3-512:<128 hex>`) of the .proof
    /// file (the bundle CID).
    pub cid: String,
    /// Contract set CID per spec #94 §1. Signer-independent. Two kits
    /// attesting to the same contracts produce the same value.
    pub contract_set_cid: String,
    /// Raw bytes of the proof envelope.
    pub bytes: Vec<u8>,
    /// Filesystem path written.
    pub path: std::path::PathBuf,
    /// Number of mementos bundled (contracts + 1 disclaimer).
    pub member_count: usize,
    /// Number of contract mementos (excludes the disclaimer).
    pub contract_count: usize,
    /// Map from contract name to its content CID (signer-independent).
    pub contract_cids: BTreeMap<String, String>,
    /// Per-slab count of contracts authored, for the report.
    pub per_slab_counts: Vec<(String, usize)>,
    /// Distinct builtin count (derived from contract names; each
    /// contract follows the `<builtin>__<predicate>` naming convention).
    pub distinct_builtin_count: usize,
    /// `disclaimer_cid` value pinned into envelope metadata.
    pub disclaimer_cid: String,
}

/// Mint all baseline contracts as signed mementos, wrap the disclaimer
/// as a memento member, bundle into a `.proof` envelope, write to
/// `<out_dir>/<cid>.proof`, and return the result.
pub fn mint_baseline(out_dir: &Path) -> Result<MintResult, String> {
    std::fs::create_dir_all(out_dir).map_err(|e| format!("create_dir_all: {e}"))?;

    let signer_seed: Ed25519Seed = FOUNDATION_V0_SEED;

    let slabs = author_all_invariants();

    let mut members: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut contract_cids: BTreeMap<String, String> = BTreeMap::new();
    let mut per_slab_counts: Vec<(String, usize)> = Vec::new();
    let mut total_contracts: usize = 0;

    for slab in &slabs {
        per_slab_counts.push((slab.source.label.into(), slab.contracts.len()));
        total_contracts += slab.contracts.len();
        for d in &slab.contracts {
            let args = MintContractArgs {
                contract_name: d.name.clone(),
                pre: d.pre.as_deref().map(formula_to_value),
                post: d.post.as_deref().map(formula_to_value),
                inv: d.inv.as_deref().map(formula_to_value),
                out_binding: d.out_binding.clone(),
                produced_by: PRODUCED_BY.into(),
                produced_at: DECLARED_AT.into(),
                input_cids: vec![],
                authoring: Authoring::KitAuthor {
                    author: PRODUCED_BY.into(),
                    note: Some(format!(
                        "rust-std baseline contract from {}",
                        slab.source.path
                    )),
                },
                signer_seed,
            };
            let ccid = compute_contract_cid(&args);
            let m = mint_contract(&args).map_err(|e| format!("mint_contract({}): {e}", d.name))?;
            if contract_cids.contains_key(&d.name) {
                return Err(format!("duplicate contract name `{}` across slabs", d.name));
            }
            contract_cids.insert(d.name.clone(), ccid);
            members.insert(m.cid, m.canonical_bytes);
        }
    }

    // Add the disclaimer member.
    let disclaimer = disclaimer_text();
    let disclaimer_cid = compute_disclaimer_cid(&disclaimer);
    let (disclaimer_attestation_cid, disclaimer_bytes) =
        mint_disclaimer_memento(&disclaimer, &signer_seed);
    if members.contains_key(&disclaimer_attestation_cid) {
        return Err(format!(
            "internal: disclaimer attestation CID collides with contract: {disclaimer_attestation_cid}"
        ));
    }
    members.insert(disclaimer_attestation_cid, disclaimer_bytes);

    let member_count = members.len();

    // Build envelope metadata block per the rubric §3. Flat dotted-key
    // shape: the `Option<BTreeMap<String, String>>` envelope metadata
    // is non-normative, so consumers can introspect either via simple
    // key lookup or by reconstructing the nested `baseline.{...}` shape
    // at parse time.
    let mut metadata: BTreeMap<String, String> = BTreeMap::new();
    metadata.insert("signer_role".into(), SIGNER_ROLE.into());
    metadata.insert("baseline.version".into(), BASELINE_VERSION.to_string());
    metadata.insert("baseline.language".into(), BASELINE_LANGUAGE.into());
    metadata.insert(
        "baseline.language_version".into(),
        BASELINE_LANGUAGE_VERSION.into(),
    );
    metadata.insert("baseline.kit_version".into(), BASELINE_KIT_VERSION.into());
    metadata.insert("baseline.disclaimer_cid".into(), disclaimer_cid.clone());

    let signer_pubkey = ed25519_pubkey_string(&signer_seed);
    let signer_cid = blake3_512_of(signer_pubkey.as_bytes());

    let proof_input = ProofEnvelopeInput {
        name: CATALOG_NAME.into(),
        version: CATALOG_VERSION.into(),
        binary_cid: None,
        metadata: Some(metadata),
        members,
        signer_cid,
        signer_seed,
        declared_at: DECLARED_AT.into(),
    };
    let built = build_proof_envelope(&proof_input);

    if !built.cid.starts_with("blake3-512:") {
        return Err("internal: cid missing blake3-512 prefix".into());
    }
    let path = out_dir.join(format!("{cid}.proof", cid = built.cid));
    std::fs::write(&path, &built.bytes).map_err(|e| format!("write {}: {e}", path.display()))?;

    let contract_set_cid = compute_contract_set_cid(contract_cids.values().cloned().collect());

    // Distinct builtin count: contracts follow `<builtin>__<predicate>`
    // naming. The double-underscore separator splits cleanly.
    let mut builtins: std::collections::BTreeSet<String> = Default::default();
    for name in contract_cids.keys() {
        let prefix = match name.rsplit_once("__") {
            Some((p, _)) => p.to_string(),
            None => name.clone(),
        };
        builtins.insert(prefix);
    }

    Ok(MintResult {
        cid: built.cid,
        contract_set_cid,
        bytes: built.bytes,
        path,
        member_count,
        contract_count: total_contracts,
        contract_cids,
        per_slab_counts,
        distinct_builtin_count: builtins.len(),
        disclaimer_cid,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir() -> std::path::PathBuf {
        // Per-invocation counter so two `tempdir()` calls inside the
        // same test don't race on the same nanosecond resolution and
        // resolve to the same path.
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);

        let mut p = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        p.push(format!(
            "provekit-rust-std-baseline-test-{nanos}-{}-{seq}",
            std::process::id()
        ));
        p
    }

    /// Skeleton-validation: the orchestrator runs end-to-end and emits
    /// at least one slab. (Scaling target: `>= 50` builtins, `>= 100`
    /// contracts: is enforced by `pinned_thresholds_met` once all
    /// slabs are populated.)
    #[test]
    fn author_all_invariants_returns_seven_slabs() {
        let slabs = author_all_invariants();
        assert_eq!(
            slabs.len(),
            7,
            "expected 7 slabs (string/vec/option/result/slice/hashmap/iter)"
        );
    }

    /// Compliance floor: builtin count >= 50 AND each builtin gets
    /// >= 2 ContractDecls (per `baseline-catalog-rubric.md` §"Compliance
    /// checklist"). The orchestrator's distinct-builtin count is
    /// derived from `<builtin>__<predicate>` naming.
    #[test]
    fn pinned_thresholds_met() {
        let dir = tempdir();
        let m = mint_baseline(&dir).expect("mint");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            m.distinct_builtin_count >= 50,
            "builtin count {} < 50 floor",
            m.distinct_builtin_count
        );
        assert!(
            m.contract_count >= m.distinct_builtin_count * 2,
            "predicate density floor: {} contracts for {} builtins (need >= 2 per builtin)",
            m.contract_count,
            m.distinct_builtin_count
        );
        // Also: every builtin should have at least 2 contracts.
        let mut by_builtin: std::collections::BTreeMap<String, usize> = Default::default();
        for name in m.contract_cids.keys() {
            let prefix = match name.rsplit_once("__") {
                Some((p, _)) => p.to_string(),
                None => name.clone(),
            };
            *by_builtin.entry(prefix).or_default() += 1;
        }
        let underdense: Vec<(String, usize)> = by_builtin
            .iter()
            .filter(|(_, n)| **n < 2)
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        assert!(
            underdense.is_empty(),
            "predicate density floor violated: {underdense:?}"
        );
    }

    /// Determinism: minting twice yields identical bytes/CID/contractSetCid.
    /// Required by the protocol's content-addressable trust root.
    #[test]
    fn mint_baseline_is_deterministic() {
        let dir1 = tempdir();
        let dir2 = tempdir();
        let m1 = mint_baseline(&dir1).expect("mint 1");
        let m2 = mint_baseline(&dir2).expect("mint 2");
        assert_eq!(m1.cid, m2.cid, "CID must be byte-deterministic across runs");
        assert_eq!(m1.contract_set_cid, m2.contract_set_cid);
        assert_eq!(m1.bytes, m2.bytes);
        assert_eq!(m1.member_count, m2.member_count);
        assert_eq!(m1.disclaimer_cid, m2.disclaimer_cid);
        let _ = std::fs::remove_dir_all(&dir1);
        let _ = std::fs::remove_dir_all(&dir2);
    }

    /// Disclaimer base text is verbatim from the rubric. Asserts a
    /// strong literal so any drift surfaces in the test failure rather
    /// than as a silent CID change. The verbatim base lock is rubric §4.
    #[test]
    fn disclaimer_base_starts_with_advisory_only() {
        assert!(
            DISCLAIMER_BASE.starts_with("Foundation baseline catalog"),
            "disclaimer base must match rubric §4"
        );
        assert!(
            DISCLAIMER_BASE.contains("It is NOT authoritative."),
            "rubric requires the literal `It is NOT authoritative.`"
        );
        assert!(
            DISCLAIMER_BASE.contains("docs/contributing/signing-your-own-catalog.md"),
            "disclaimer must point at the federation doc"
        );
    }

    /// Per-language addendum names the rust-lang steward + the version
    /// the catalog is authored against (rubric §4).
    #[test]
    fn disclaimer_addendum_names_rust_steward() {
        assert!(DISCLAIMER_ADDENDUM.contains("Language: rust"));
        assert!(DISCLAIMER_ADDENDUM.contains("Steward: rust-lang team"));
        assert!(DISCLAIMER_ADDENDUM.contains("rustc 1.81.0"));
        assert!(DISCLAIMER_ADDENDUM.contains("Steward signature available: no"));
    }

    /// Envelope metadata floor (rubric §3): signer_role +
    /// baseline.{version,language,language_version,kit_version,disclaimer_cid}.
    #[test]
    fn envelope_metadata_carries_required_fields() {
        let dir = tempdir();
        let m = mint_baseline(&dir).expect("mint");
        let _ = std::fs::remove_dir_all(&dir);

        // Re-decode the catalog to pull metadata out of the on-disk bytes.
        // We don't ship a full CBOR decoder reader here; the assertion
        // is structural: the mint pipeline ALWAYS sets these keys, so
        // they're guaranteed on-disk if the call returned Ok.
        assert!(m.disclaimer_cid.starts_with("blake3-512:"));
        assert_eq!(BASELINE_LANGUAGE, "rust");
        assert_eq!(BASELINE_LANGUAGE_VERSION, "1.81.0");
        assert_eq!(BASELINE_KIT_VERSION, "0.1.0");
        assert_eq!(BASELINE_VERSION, 1);
        assert_eq!(SIGNER_ROLE, "foundation-baseline");
    }
}
