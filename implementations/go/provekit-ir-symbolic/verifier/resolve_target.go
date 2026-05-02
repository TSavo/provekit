package verifier

import (
	"fmt"
	"os"
)

// ResolveTargetStage hash-looks-up a bridge's targetContractCid in the
// pool and returns the resolved contract memento's `pre` formula. O(1):
// no file IO; pool was built once at LoadAllProofsStage.
//
// v1.1.0 cut: the bridge target is a CONTRACT memento; the consumer-side
// discharge targets the contract's `pre` slot (the precondition the
// caller must establish at the call site). post/inv participate in the
// handshake algorithm via their own slots.
//
// Forward-pin gate (BridgeDeclaration.ConsequentBundlePinned, NORMATIVE):
// after locating the consequent contract member, refuse to consume it
// unless its containing `.proof` bundle CID matches the bridge's
// `targetProofCid`. See protocol/specs/2026-04-30-ir-formal-grammar.md
// § "Bridge target pinning: the shim-poisoning vector". Mirrors Rust
// PR #13 (provekit-verifier/src/resolve_target.rs lines 32-69).
type ResolveTargetStage struct{}

// Run resolves a single CallSite's bridge target.
func (s *ResolveTargetStage) Run(cs CallSite, pool *MementoPool) (*ResolvedProperty, string) {
	env, ok := pool.Mementos[cs.BridgeTargetCID]
	if !ok {
		return nil, "not-in-pool"
	}
	ev, ok := env["evidence"].(map[string]interface{})
	if !ok {
		return nil, "evidence-missing"
	}
	if ev["kind"] != "contract" {
		return nil, "not-contract-variant"
	}
	body, _ := ev["body"].(map[string]interface{})
	if body == nil {
		return nil, "body-missing"
	}

	// Forward pin: BridgeDeclaration.ConsequentBundlePinned.
	//
	//     ∀b: BridgeDeclaration, P: ProofBundle →
	//       AcceptedAsConsequentFor(P, b) ⇒ Cid(P) = b.targetProofCid
	//
	// If the bridge pins a target proof CID, the contract member we
	// just resolved MUST come from that bundle. Bundles whose contract
	// members happen to share `targetContractCid` MUST NOT be
	// substituted for the pinned bundle. Mirrors Rust resolve_target.rs
	// lines 43-69.
	if expectedBundle := cs.BridgeTargetProofCID; expectedBundle != "" {
		members, ok := pool.BundleMembers[expectedBundle]
		if !ok {
			return nil, fmt.Sprintf(
				"BridgeTargetProofCidMismatch: pinned bundle %s not in pool",
				expectedBundle,
			)
		}
		if _, ok := members[cs.BridgeTargetCID]; !ok {
			return nil, fmt.Sprintf(
				"BridgeTargetProofCidMismatch: contract %s is not a member of pinned bundle %s",
				cs.BridgeTargetCID, expectedBundle,
			)
		}
	} else {
		// Back-compat: legacy bridges that pre-date `targetProofCid`
		// are loadable but cannot have ConsequentBundlePinned
		// enforced. New bridges MUST set the field; flag the gap so
		// operators can see what isn't being checked. Mirrors Rust's
		// `eprintln!("warning: ...")` path.
		fmt.Fprintf(os.Stderr,
			"warning: bridge %s has no targetProofCid; "+
				"ConsequentBundlePinned not enforced (back-compat path)\n",
			cs.BridgeIRName)
	}

	pre, ok := body["pre"]
	if !ok {
		return nil, "no-pre-slot"
	}
	return &ResolvedProperty{
		CID:       cs.BridgeTargetCID,
		IRFormula: pre,
	}, ""
}
