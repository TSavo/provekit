// SPDX-License-Identifier: Apache-2.0
//
// Stage 3 (resolve_target) tests. Pins:
//   - looks up the bridge's targetCid in the pool's mementos
//   - kind == "contract" required (fail-closed on other kinds)
//   - reads body.pre as the discharge target
//   - fail-closed when targetCid is not in the pool
//   - fail-closed when the resolved memento has no body
//   - returns the memento's CID in `cid`

use serde_json::{json, Value as Json};

use provekit_verifier::{resolve_target, CallSite, MementoPool};

fn pool_with(cid: &str, env: Json) -> MementoPool {
    let mut pool = MementoPool::default();
    pool.mementos.insert(cid.into(), env);
    pool
}

fn callsite_targeting(target_cid: &str) -> CallSite {
    CallSite {
        bridge_ir_name: "parseInt".into(),
        bridge_target_cid: target_cid.into(),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Happy path
// ---------------------------------------------------------------------------

#[test]
fn resolves_pre_for_contract_memento() {
    let target_cid = "blake3-512:contract1";
    let pre = json!({
        "kind": "forall",
        "name": "n",
        "sort": {"kind": "primitive", "name": "Int"},
        "body": {
            "kind": "atomic", "name": ">", "args": [
                {"kind": "var", "name": "n"},
                {"kind": "const", "value": 0, "sort": {"kind": "primitive", "name": "Int"}}
            ]
        }
    });
    let env = json!({
        "evidence": {
            "kind": "contract",
            "body": {"pre": pre.clone()}
        }
    });
    let pool = pool_with(target_cid, env);
    let cs = callsite_targeting(target_cid);
    let r = resolve_target::run(&cs, &pool).expect("resolve");
    assert_eq!(r.cid, target_cid);
    assert_eq!(r.ir_formula, Some(pre));
}

#[test]
fn resolves_returns_none_pre_when_contract_has_no_pre() {
    let target_cid = "blake3-512:contract1";
    let env = json!({
        "evidence": {
            "kind": "contract",
            "body": {"post": {"kind": "atomic", "name": "=", "args": []}}
        }
    });
    let pool = pool_with(target_cid, env);
    let cs = callsite_targeting(target_cid);
    let r = resolve_target::run(&cs, &pool).expect("resolve");
    assert!(r.ir_formula.is_none());
}

// ---------------------------------------------------------------------------
// Fail-closed: bad inputs
// ---------------------------------------------------------------------------

#[test]
fn errors_when_target_cid_not_in_pool() {
    let pool = MementoPool::default();
    let cs = callsite_targeting("blake3-512:nope");
    let r = resolve_target::run(&cs, &pool);
    assert!(r.is_err(), "must fail-closed when target missing");
    let err = format!("{:?}", r.err().unwrap());
    assert!(err.contains("not in pool"));
}

#[test]
fn errors_when_target_kind_is_bridge_not_contract() {
    let target_cid = "blake3-512:bridge1";
    let env = json!({
        "evidence": {
            "kind": "bridge",
            "body": {"sourceSymbol": "parseInt"}
        }
    });
    let pool = pool_with(target_cid, env);
    let cs = callsite_targeting(target_cid);
    let r = resolve_target::run(&cs, &pool);
    assert!(r.is_err());
    let err = format!("{:?}", r.err().unwrap());
    assert!(err.contains("not a contract"));
}

#[test]
fn errors_when_target_kind_is_implication() {
    let target_cid = "blake3-512:impl1";
    let env = json!({
        "evidence": {
            "kind": "implication",
            "body": {}
        }
    });
    let pool = pool_with(target_cid, env);
    let cs = callsite_targeting(target_cid);
    let r = resolve_target::run(&cs, &pool);
    assert!(r.is_err());
}

#[test]
fn errors_when_evidence_is_missing() {
    let target_cid = "blake3-512:bad1";
    let env = json!({"otherStuff": "no evidence"});
    let pool = pool_with(target_cid, env);
    let cs = callsite_targeting(target_cid);
    let r = resolve_target::run(&cs, &pool);
    assert!(r.is_err());
}

#[test]
fn errors_when_contract_body_is_missing() {
    let target_cid = "blake3-512:contract2";
    let env = json!({
        "evidence": {"kind": "contract"}
        // no body
    });
    let pool = pool_with(target_cid, env);
    let cs = callsite_targeting(target_cid);
    let r = resolve_target::run(&cs, &pool);
    assert!(r.is_err());
}

#[test]
fn errors_when_evidence_kind_is_unknown() {
    let target_cid = "blake3-512:c";
    let env = json!({
        "evidence": {"kind": "weird-kind", "body": {"pre": {}}}
    });
    let pool = pool_with(target_cid, env);
    let cs = callsite_targeting(target_cid);
    let r = resolve_target::run(&cs, &pool);
    assert!(r.is_err());
}
