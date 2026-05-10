// SPDX-License-Identifier: Apache-2.0
//
// Acceptance test for the headline `provekit must` UX:
//
//   provekit must doubleledger.ts "not lose money" --agent stub
//
// The stub agent recognises the phrase, returns the
// double-entry conservation invariant in canonical IR-JSON, and the
// validation+mint loop produces a signed memento.

use std::path::PathBuf;

use provekit_agent::{run_must_loop, MustContext, MustLoopOptions, StubAgent};

fn fixture_path() -> PathBuf {
    // CARGO_MANIFEST_DIR points at the provekit-agent crate directory:
    //   <repo>/implementations/rust/provekit-agent
    // The fixture lives at:
    //   <repo>/examples/agent-plugins/doubleledger-fixture/src/doubleledger.ts
    // so we hop up three levels (provekit-agent → rust → implementations → repo).
    let manifest = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest)
        .join("../../../examples/agent-plugins/doubleledger-fixture/src/doubleledger.ts")
}

#[test]
fn doubleledger_must_produces_conservation_contract() {
    let path = fixture_path();
    assert!(
        path.exists(),
        "fixture missing at {}: examples/agent-plugins/doubleledger-fixture must exist",
        path.display()
    );
    let source_text = std::fs::read_to_string(&path).expect("read fixture");

    let agent = StubAgent::new();
    let ctx = MustContext {
        source_path: path.clone(),
        source_text,
        description: "not lose money".into(),
        authoring_api_doc: String::new(),
        previous_rejection: None,
    };
    let outcome = run_must_loop(&agent, ctx, &MustLoopOptions::default()).expect("must loop");

    // The stub agent recognises the phrase and returns the conservation
    // contract by name.
    assert_eq!(outcome.candidate.name, "doubleledger_conservation");

    // The minted memento carries a real BLAKE3-512 CID.
    assert!(
        outcome.minted.cid.starts_with("blake3-512:"),
        "expected blake3-512 CID, got {}",
        outcome.minted.cid
    );
    // CIDs are 11 (prefix) + 128 (hex digest) = 139 chars.
    assert_eq!(
        outcome.minted.cid.len(),
        11 + 128,
        "cid: {}",
        outcome.minted.cid
    );

    // The contract is an invariant (not pre/post).
    assert!(outcome.candidate.inv.is_some());
    assert!(outcome.candidate.pre.is_none());
    assert!(outcome.candidate.post.is_none());

    // The IR-JSON for the invariant references both sumDebits and sumCredits
    // over the bound transaction variable.
    let inv = outcome.candidate.inv.as_deref().unwrap();
    assert!(inv.contains("sumDebits"));
    assert!(inv.contains("sumCredits"));
    assert!(inv.contains("forall"));
    assert!(inv.contains("\"name\":\"txn\""));

    // No rejections necessary; first try lands.
    assert_eq!(outcome.rejected.len(), 0);
    assert_eq!(outcome.agent_calls, 1);

    // The minted bytes are non-empty (signed claim envelope).
    assert!(!outcome.minted.canonical_bytes.is_empty());
}

#[test]
fn doubleledger_minted_bytes_round_trip_via_canonical_form() {
    let path = fixture_path();
    let source_text = std::fs::read_to_string(&path).expect("read fixture");
    let agent = StubAgent::new();
    let ctx = MustContext {
        source_path: path,
        source_text,
        description: "not lose money".into(),
        authoring_api_doc: String::new(),
        previous_rejection: None,
    };
    let o1 = run_must_loop(&agent, ctx.clone(), &MustLoopOptions::default()).expect("a");
    let o2 = run_must_loop(&agent, ctx, &MustLoopOptions::default()).expect("b");
    assert_eq!(
        o1.minted.cid, o2.minted.cid,
        "deterministic mint must produce the same CID"
    );
    assert_eq!(o1.minted.canonical_bytes, o2.minted.canonical_bytes);
}
