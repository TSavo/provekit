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

use provekit_claim_envelope::{
    mint_contract, Authoring, ClaimEnvelopeError, MintContractArgs, MintedEnvelope,
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
}
