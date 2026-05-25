// SPDX-License-Identifier: Apache-2.0
//
// Wrap a `FunctionContractMemento` as a signed layered MintedEnvelope.
// Per #372 part 2.
//
// `provekit-claim-envelope::mint_contract` is the substrate's canonical
// path for emitting contract mementos: it produces the v1.2 layered
// shape `{envelope, header, metadata}` with an Ed25519 attestation
// signature embedded in the envelope. The minted CID is the
// attestation CID; the header carries the signer-independent
// `contract_cid`.
//
// This module is the converter from walk's internal contract type to
// the kit's `MintContractArgs`. Once a contract is wrapped, it plugs
// into the proof.ir bundle pipeline and the resolve/index
// substrate-verifier path with no further translation.

use std::collections::HashMap;

use provekit_claim_envelope::{
    contract_cid as kit_contract_cid, mint_contract, Authoring, ClaimEnvelopeError,
    MintContractArgs, MintedEnvelope,
};
use provekit_proof_envelope::Ed25519Seed;

use crate::canonical::formula_to_canonical;
use crate::contract::FunctionContractMemento;

/// Default development signer seed. Production callers must supply a
/// vault-backed seed; this seed is for tests, demos, and self-attested
/// dogfood emission only.
pub const DEV_SIGNER_SEED: Ed25519Seed = [0x42; 32];

/// Wrap a FunctionContractMemento as a signed layered MintedEnvelope.
/// `produced_at` is the RFC-3339 declaration timestamp embedded in the
/// envelope (also re-used as `producedAt` in the metadata block).
pub fn wrap_function_contract(
    contract: &FunctionContractMemento,
    produced_at: &str,
    signer_seed: &Ed25519Seed,
) -> Result<MintedEnvelope, ClaimEnvelopeError> {
    let args = mint_args(contract, produced_at, signer_seed);
    mint_contract(&args)
}

/// Content-addressed envelope cache + mint counter. Per #368 AC #6:
/// "Second invocation hits cache (no re-mint, demonstrated via
/// mint-counter assertion)" — paper 07 §6's "compose for free,
/// compress to nothing" empirically.
///
/// Lookups are by `contract_cid` (signer-independent). The cache
/// returns the previously-minted MintedEnvelope unchanged on hit;
/// callers receive the same `cid` and `canonical_bytes` they would
/// have re-minted, but without paying the Ed25519 signing cost.
#[derive(Debug, Default)]
pub struct EnvelopeCache {
    /// contract_cid → cached MintedEnvelope
    by_contract: HashMap<String, MintedEnvelope>,
    /// Number of times mint_contract was actually invoked.
    pub mints: u64,
    /// Number of times the cache served a previously-minted envelope.
    pub hits: u64,
}

impl EnvelopeCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.by_contract.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_contract.is_empty()
    }
}

/// Wrap a FunctionContractMemento with a content-addressed cache: if the
/// signer-independent `contract_cid` has already been minted into this
/// cache, return the cached envelope (incrementing `cache.hits`);
/// otherwise mint, insert, and return (incrementing `cache.mints`).
pub fn wrap_function_contract_cached(
    contract: &FunctionContractMemento,
    produced_at: &str,
    signer_seed: &Ed25519Seed,
    cache: &mut EnvelopeCache,
) -> Result<MintedEnvelope, ClaimEnvelopeError> {
    let args = mint_args(contract, produced_at, signer_seed);
    let cid = kit_contract_cid(&args);
    if let Some(env) = cache.by_contract.get(&cid) {
        cache.hits += 1;
        return Ok(env.clone());
    }
    let env = mint_contract(&args)?;
    cache.mints += 1;
    cache.by_contract.insert(cid, env.clone());
    Ok(env)
}

/// Build the `MintContractArgs` from a contract memento. Exposed so
/// callers (and tests) can compute the signer-independent `contract_cid`
/// via `provekit_claim_envelope::contract_cid(&args)` without paying the
/// signing cost.
pub fn mint_args(
    contract: &FunctionContractMemento,
    produced_at: &str,
    signer_seed: &Ed25519Seed,
) -> MintContractArgs {
    let pre = formula_to_canonical(&contract.pre);
    let post = formula_to_canonical(&contract.post);
    let out_binding = contract.result_var_name();

    let input_cids: Vec<String> = contract
        .body_cid
        .as_ref()
        .map(|c| vec![c.clone()])
        .unwrap_or_default();

    // Carry the body-derived op-contract's formals (+ sorts) into the
    // minted header so `body_discharge::CatalogResolver` can resolve the
    // body-obligation (#1436/#1440). walk's `post` already equates
    // `result == <body-expr>`, so with formals present the contract is a
    // complete body-discharge target.
    let formals = contract.formals.clone();
    let formal_sorts = contract
        .formal_sorts
        .iter()
        .map(|s| {
            let json = serde_json::to_value(s).unwrap_or(serde_json::Value::Null);
            crate::canonical::serde_to_canonical(json)
        })
        .collect();

    MintContractArgs {
        contract_name: contract.fn_name.clone(),
        pre: Some(pre),
        post: Some(post),
        inv: None,
        out_binding,
        produced_by: "provekit-walk".to_string(),
        produced_at: produced_at.to_string(),
        input_cids,
        authoring: Authoring::Lift {
            lifter: "provekit-walk".to_string(),
            evidence: "syn-walk-v1".to_string(),
            source_cid: contract.body_cid.clone(),
        },
        signer_seed: *signer_seed,
        formals,
        emit_empty_formals: contract.formals.is_empty(),
        formal_sorts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::build_function_contract;
    use provekit_claim_envelope::contract_cid as kit_contract_cid;

    fn fixture_contract(src: &str) -> FunctionContractMemento {
        let file: syn::File = syn::parse_str(src).unwrap();
        let item_fn = file
            .items
            .into_iter()
            .find_map(|item| match item {
                syn::Item::Fn(f) => Some(f),
                _ => None,
            })
            .unwrap();
        build_function_contract(&item_fn, None)
    }

    #[test]
    fn wrap_emits_layered_envelope() {
        let c = fixture_contract("fn inc(x: i64) -> i64 { x + 1 }");
        let env = wrap_function_contract(&c, "2026-05-04T00:00:00Z", &DEV_SIGNER_SEED).unwrap();

        // Attestation CID is non-empty and well-formed.
        assert!(!env.cid.is_empty());
        assert!(env.cid.starts_with("blake3-512:"));

        // Contract CID is populated (signer-independent).
        assert!(!env.contract_cid.is_empty());
        assert!(env.contract_cid.starts_with("blake3-512:"));

        // Canonical bytes parse as a layered shape.
        let s = std::str::from_utf8(&env.canonical_bytes).unwrap();
        assert!(s.contains("\"envelope\""));
        assert!(s.contains("\"header\""));
        assert!(s.contains("\"metadata\""));
        assert!(s.contains("\"kind\":\"contract\""));
        assert!(s.contains("\"schemaVersion\":\"2\""));
    }

    #[test]
    fn wrap_is_deterministic_for_same_inputs() {
        let c = fixture_contract("fn id(x: i64) -> i64 { x }");
        let a = wrap_function_contract(&c, "2026-05-04T00:00:00Z", &DEV_SIGNER_SEED).unwrap();
        let b = wrap_function_contract(&c, "2026-05-04T00:00:00Z", &DEV_SIGNER_SEED).unwrap();
        assert_eq!(a.cid, b.cid);
        assert_eq!(a.contract_cid, b.contract_cid);
        assert_eq!(a.canonical_bytes, b.canonical_bytes);
    }

    #[test]
    fn different_signers_share_contract_cid() {
        // The contract_cid is signer-independent — two signers attesting
        // to the same logical contract must produce the same content
        // CID, even though their attestation CIDs differ.
        let c = fixture_contract("fn add(x: i64) -> i64 { x + 2 }");
        let seed_a: Ed25519Seed = [0x11; 32];
        let seed_b: Ed25519Seed = [0x22; 32];
        let env_a = wrap_function_contract(&c, "2026-05-04T00:00:00Z", &seed_a).unwrap();
        let env_b = wrap_function_contract(&c, "2026-05-04T00:00:00Z", &seed_b).unwrap();

        assert_eq!(
            env_a.contract_cid, env_b.contract_cid,
            "contract_cid must be signer-independent"
        );
        assert_ne!(
            env_a.cid, env_b.cid,
            "attestation cids must differ across signers"
        );
        assert_ne!(env_a.canonical_bytes, env_b.canonical_bytes);
    }

    #[test]
    fn distinct_functions_produce_distinct_contract_cids() {
        let c1 = fixture_contract("fn one(x: i64) -> i64 { x + 1 }");
        let c2 = fixture_contract("fn two(x: i64) -> i64 { x + 2 }");
        let e1 = wrap_function_contract(&c1, "2026-05-04T00:00:00Z", &DEV_SIGNER_SEED).unwrap();
        let e2 = wrap_function_contract(&c2, "2026-05-04T00:00:00Z", &DEV_SIGNER_SEED).unwrap();
        assert_ne!(e1.contract_cid, e2.contract_cid);
        assert_ne!(e1.cid, e2.cid);
    }

    #[test]
    fn mint_args_contract_cid_matches_wrap() {
        // contract_cid(args) must equal the embedded header.cid in the
        // minted envelope, so callers can compute it cheaply without
        // signing.
        let c = fixture_contract("fn neg(x: i64) -> i64 { -x }");
        let args = mint_args(&c, "2026-05-04T00:00:00Z", &DEV_SIGNER_SEED);
        let cid_via_args = kit_contract_cid(&args);
        let env = wrap_function_contract(&c, "2026-05-04T00:00:00Z", &DEV_SIGNER_SEED).unwrap();
        assert_eq!(cid_via_args, env.contract_cid);
    }

    // ---- AC #6: mint-counter cache assertion ----

    #[test]
    fn cached_wrap_first_invocation_mints_second_hits() {
        // Closes #368 AC #6: "Second invocation hits cache (no re-mint,
        // demonstrated via mint-counter assertion)."
        let c = fixture_contract("fn inc(x: i64) -> i64 { x + 1 }");
        let mut cache = EnvelopeCache::new();
        assert_eq!(cache.mints, 0);
        assert_eq!(cache.hits, 0);

        // First invocation: mints, cache becomes non-empty.
        let env1 =
            wrap_function_contract_cached(&c, "2026-05-05T00:00:00Z", &DEV_SIGNER_SEED, &mut cache)
                .unwrap();
        assert_eq!(cache.mints, 1, "first call must mint");
        assert_eq!(cache.hits, 0);
        assert_eq!(cache.len(), 1);

        // Second invocation on the SAME source: hits the cache, mint counter
        // does not increment.
        let env2 =
            wrap_function_contract_cached(&c, "2026-05-05T00:00:00Z", &DEV_SIGNER_SEED, &mut cache)
                .unwrap();
        assert_eq!(cache.mints, 1, "second call must NOT re-mint");
        assert_eq!(cache.hits, 1, "second call must register a hit");
        assert_eq!(env1.cid, env2.cid);
        assert_eq!(env1.canonical_bytes, env2.canonical_bytes);
        assert_eq!(env1.contract_cid, env2.contract_cid);
    }

    #[test]
    fn cached_wrap_distinct_contracts_each_mint_once() {
        // Two distinct contracts each get minted once; cache.mints == 2,
        // hits == 0. Then re-querying both produces 2 hits.
        let c1 = fixture_contract("fn one(x: i64) -> i64 { x + 1 }");
        let c2 = fixture_contract("fn two(x: i64) -> i64 { x + 2 }");
        let mut cache = EnvelopeCache::new();

        wrap_function_contract_cached(&c1, "2026-05-05T00:00:00Z", &DEV_SIGNER_SEED, &mut cache)
            .unwrap();
        wrap_function_contract_cached(&c2, "2026-05-05T00:00:00Z", &DEV_SIGNER_SEED, &mut cache)
            .unwrap();
        assert_eq!(cache.mints, 2);
        assert_eq!(cache.hits, 0);

        wrap_function_contract_cached(&c1, "2026-05-05T00:00:00Z", &DEV_SIGNER_SEED, &mut cache)
            .unwrap();
        wrap_function_contract_cached(&c2, "2026-05-05T00:00:00Z", &DEV_SIGNER_SEED, &mut cache)
            .unwrap();
        assert_eq!(cache.mints, 2, "no re-mint");
        assert_eq!(cache.hits, 2);
    }

    #[test]
    fn cached_wrap_signer_independent_lookup() {
        // contract_cid is signer-independent. Caching by contract_cid
        // means two signers minting the same logical contract still
        // produce a cache hit on the second call.
        let c = fixture_contract("fn add(x: i64) -> i64 { x + 5 }");
        let seed_a: Ed25519Seed = [0x11; 32];
        let seed_b: Ed25519Seed = [0x22; 32];
        let mut cache = EnvelopeCache::new();

        let env_a =
            wrap_function_contract_cached(&c, "2026-05-05T00:00:00Z", &seed_a, &mut cache).unwrap();
        // Signer B asks for the same logical contract: cache returns the
        // first signer's envelope (CIDs are content-addressed; the cache
        // is signer-independent).
        let env_b =
            wrap_function_contract_cached(&c, "2026-05-05T00:00:00Z", &seed_b, &mut cache).unwrap();
        assert_eq!(cache.mints, 1, "signer-independent cache means one mint");
        assert_eq!(cache.hits, 1);
        assert_eq!(env_a.cid, env_b.cid);
    }
}
