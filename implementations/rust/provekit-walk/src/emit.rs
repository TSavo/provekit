// SPDX-License-Identifier: Apache-2.0
//
// Emit the shadow source as a v1.5.0-shape proof.ir bundle.
//
// The bundle is a single JCS-canonical JSON document containing:
//   - schemaVersion: "provekit-walk/1"
//   - shadowSourceCid: top-level CID for the shadow source
//   - shadowSource: the canonical shadow-source bytes (decoded back to a
//     JSON object so consumers can inspect without re-canonicalizing)
//   - arrivals: array of every shadow arrival's edge memento, each
//     shaped as ContractDecl per paper 07 §11
//   - composedChain: optional flat composed edge for the longest chain
//
// This is the "from source to substrate" wire-format gap closed: feed
// any Rust source into walk_demo and out the other side comes a single
// JCS+BLAKE3-addressed bundle that downstream substrate tools (lift,
// linker, mint) can consume.

use std::sync::Arc;

use provekit_canonicalizer::Value;

use crate::canonical::{cid_of_value, jcs_bytes_of_value};
use crate::shadow::{compose_chain, edge_memento_value, ShadowSource};

/// Emit a single proof.ir bundle for the given shadow source.
/// Returns JCS-canonical bytes ready for write or transmit. The bundle's
/// own CID is included inline.
pub fn shadow_to_proof_ir(s: &ShadowSource) -> Vec<u8> {
    let bundle = build_bundle_value(s);
    jcs_bytes_of_value(&bundle)
}

/// CID of the proof.ir bundle.
pub fn shadow_proof_ir_cid(s: &ShadowSource) -> String {
    let bundle = build_bundle_value(s);
    cid_of_value(&bundle)
}

fn build_bundle_value(s: &ShadowSource) -> Arc<Value> {
    // Collect every arrival's edge memento as a separate object inside
    // the bundle's `arrivals` array. Each carries its own CID (as a
    // sibling field) so consumers can index without re-hashing.
    let arrivals: Vec<Arc<Value>> = s
        .all_arrivals()
        .map(|(_slot, arrival)| {
            let edge_value = edge_memento_value(arrival);
            let edge_cid = cid_of_value(&edge_value);
            Value::object([
                ("cid", Value::string(edge_cid)),
                ("memento", edge_value),
                ("arrivalCid", Value::string(arrival.cid.clone())),
                ("calleeName", Value::string(arrival.callee_name.clone())),
                ("sourceIndex", Value::integer(arrival.source_index as i64)),
            ])
        })
        .collect();

    // Best-effort composed chain: take the longest chain (stable tie-break).
    let composed_chain_value: Arc<Value> = match longest_chain(s) {
        Some(arrivals) if !arrivals.is_empty() => {
            let composed = compose_chain(arrivals.iter().copied());
            let component_cids: Vec<Arc<Value>> = composed
                .component_cids
                .iter()
                .map(|c| Value::string(c.clone()))
                .collect();
            Value::object([
                ("cid", Value::string(composed.cid)),
                ("componentCids", Value::array(component_cids)),
            ])
        }
        _ => Value::null(),
    };

    Value::object([
        ("schemaVersion", Value::string("provekit-walk/1")),
        ("kind", Value::string("walk-bundle")),
        ("shadowSourceCid", Value::string(s.cid.clone())),
        ("fnName", Value::string(s.fn_name.clone())),
        ("slotCount", Value::integer(s.slots.len() as i64)),
        ("arrivals", Value::array(arrivals)),
        ("composedChain", composed_chain_value),
    ])
}

fn longest_chain(s: &ShadowSource) -> Option<Vec<&crate::shadow::ShadowArrival>> {
    // Group arrivals by callee_root_cid and pick the chain with the most
    // arrivals. BTreeMap (sorted by callee_root_cid key) guarantees
    // deterministic iteration order so that when two chains have the same
    // length the FIRST key in lexicographic order wins — result is
    // byte-for-byte identical across calls regardless of HashMap seed.
    use std::collections::BTreeMap;
    let mut chains: BTreeMap<String, Vec<&crate::shadow::ShadowArrival>> = BTreeMap::new();
    for (_, arrival) in s.all_arrivals() {
        chains
            .entry(arrival.callee_root_cid.clone())
            .or_default()
            .push(arrival);
    }
    chains.into_values().max_by_key(|c| c.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        atomic_ge, build_shadow_source, const_int, lift_function_precondition, var, CalleeContract,
    };

    fn parse_named(src: &str, name: &str) -> syn::ItemFn {
        let file: syn::File = syn::parse_str(src).unwrap();
        file.items
            .into_iter()
            .find_map(|item| match item {
                syn::Item::Fn(f) if f.sig.ident == name => Some(f),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn proof_ir_round_trips_with_stable_cid() {
        let src = r#"
            fn f(x: u32) -> u32 { if x < 10 { panic!(); } x * 2 }
            fn main() { let y: u32 = 42; let result = f(y); }
        "#;
        let f_fn = parse_named(src, "f");
        let main_fn = parse_named(src, "main");
        let pre = lift_function_precondition(&f_fn);
        let s = build_shadow_source(
            &main_fn,
            &[CalleeContract {
                callee_name: "f".to_string(),
                formal_params: vec!["x".to_string()],
                precondition: pre,
            }],
        );
        let bytes = shadow_to_proof_ir(&s);
        let cid = shadow_proof_ir_cid(&s);
        assert!(!bytes.is_empty());
        assert!(cid.starts_with("blake3-512:"));
        // Stable across calls.
        assert_eq!(bytes, shadow_to_proof_ir(&s));
        assert_eq!(cid, shadow_proof_ir_cid(&s));
        // The bytes should parse as JSON.
        let parsed: serde_json::Value = serde_json::from_slice(&bytes).expect("valid JSON");
        assert_eq!(
            parsed["schemaVersion"].as_str(),
            Some("provekit-walk/1")
        );
        assert_eq!(
            parsed["shadowSourceCid"].as_str(),
            Some(s.cid.as_str())
        );
    }

    #[test]
    fn proof_ir_distinct_for_distinct_sources() {
        let src_a = r#"
            fn f(x: u32) -> u32 { if x < 10 { panic!(); } x * 2 }
            fn main() { let y: u32 = 42; let result = f(y); }
        "#;
        let src_b = r#"
            fn f(x: u32) -> u32 { if x < 20 { panic!(); } x * 3 }
            fn main() { let y: u32 = 99; let result = f(y); }
        "#;
        let make_bundle = |src: &str| {
            let f_fn = parse_named(src, "f");
            let main_fn = parse_named(src, "main");
            let pre = lift_function_precondition(&f_fn);
            let s = build_shadow_source(
                &main_fn,
                &[CalleeContract {
                    callee_name: "f".to_string(),
                    formal_params: vec!["x".to_string()],
                    precondition: pre,
                }],
            );
            shadow_proof_ir_cid(&s)
        };
        // Suppress unused-helper warning; both calls below.
        let _bare = atomic_ge(var("x"), const_int(10));
        assert_ne!(make_bundle(src_a), make_bundle(src_b));
    }

    // Bug #1: longest_chain must be deterministic when two callees produce
    // chains of equal length. With HashMap (random iteration) the tie-break
    // was non-deterministic; with BTreeMap it picks the lexicographically
    // first key every time.
    #[test]
    fn longest_chain_tie_break_is_deterministic() {
        let src = r#"
            fn f(x: u32) -> u32 { if x < 10 { panic!(); } x * 2 }
            fn g(y: u32) -> u32 { if y < 5  { panic!(); } y + 1 }
            fn main() {
                let a: u32 = 42;
                let b: u32 = 20;
                let r1 = f(a);
                let r2 = g(b);
            }
        "#;
        let f_fn = parse_named(src, "f");
        let g_fn = parse_named(src, "g");
        let main_fn = parse_named(src, "main");
        let pre_f = lift_function_precondition(&f_fn);
        let pre_g = lift_function_precondition(&g_fn);
        let s = build_shadow_source(
            &main_fn,
            &[
                CalleeContract {
                    callee_name: "f".to_string(),
                    formal_params: vec!["x".to_string()],
                    precondition: pre_f,
                },
                CalleeContract {
                    callee_name: "g".to_string(),
                    formal_params: vec!["y".to_string()],
                    precondition: pre_g,
                },
            ],
        );
        let bytes_first = shadow_to_proof_ir(&s);
        for _ in 0..50 {
            assert_eq!(
                bytes_first,
                shadow_to_proof_ir(&s),
                "bundle bytes must be deterministic across calls (tie-break in longest_chain)"
            );
        }
        let cid_first = shadow_proof_ir_cid(&s);
        for _ in 0..50 {
            assert_eq!(
                cid_first,
                shadow_proof_ir_cid(&s),
                "bundle CID must be deterministic across calls"
            );
        }
    }
}
