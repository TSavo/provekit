// SPDX-License-Identifier: Apache-2.0
//
// Stage 3 (resolve_target) tests for the forward pin
// BridgeDeclaration.ConsequentBundlePinned (NORMATIVE).
//
// Mirrors implementations/rust/provekit-verifier/tests/resolve_target.rs
// (PR #13). Same fixtures, same verdicts: cross-impl conformance is
// the contract.
//
// Spec: protocol/specs/2026-04-30-ir-formal-grammar.md
//   § "Bridge target pinning: the shim-poisoning vector"
//
//     ∀b: BridgeDeclaration, P: ProofBundle →
//       AcceptedAsConsequentFor(P, b) ⇒ Cid(P) = b.targetProofCid

package verifier

import (
	"strings"
	"testing"
)

// trivialPre returns a minimal `pre` formula so the resolver can
// produce a ResolvedProperty when the pin check passes.
func trivialPre() map[string]interface{} {
	return map[string]interface{}{
		"kind": "atomic",
		"name": "true",
		"args": []interface{}{},
	}
}

// contractEnv builds a minimal contract envelope around `pre`.
func contractEnv(pre map[string]interface{}) map[string]interface{} {
	return map[string]interface{}{
		"evidence": map[string]interface{}{
			"kind": "contract",
			"body": map[string]interface{}{"pre": pre},
		},
	}
}

// poolWith returns a fresh pool seeded with one memento at cid.
func poolWith(cid string, env map[string]interface{}) *MementoPool {
	pool := NewMementoPool()
	pool.Mementos[cid] = env
	return pool
}

// rejects_when_target_proof_cid_does_not_match_bundle (Rust analog).
//
// The contract member exists in the pool, but it was loaded from a
// different `.proof` bundle than the bridge pinned. The verifier MUST
// reject with `BridgeTargetProofCidMismatch`. This is the
// shim-poisoning attack the spec section closes.
func TestResolveTarget_RejectsWhenTargetProofCidDoesNotMatchBundle(t *testing.T) {
	targetCID := "blake3-512:contract-shared"
	honestBundle := "blake3-512:node-v24-proof-honest"
	poisonedBundle := "blake3-512:node-v24-proof-poisoned"

	pool := poolWith(targetCID, contractEnv(trivialPre()))
	// Member was loaded as part of the poisoned bundle. The honest
	// bundle is what the bridge pinned but isn't present.
	pool.BundleMembers[poisonedBundle] = map[string]struct{}{
		targetCID: {},
	}

	cs := CallSite{
		BridgeIRName:         "parseInt",
		BridgeTargetCID:      targetCID,
		BridgeTargetProofCID: honestBundle,
	}

	stage := &ResolveTargetStage{}
	resolved, reason := stage.Run(cs, pool)
	if resolved != nil {
		t.Fatalf("must reject, got resolved=%+v reason=%q", resolved, reason)
	}
	if !strings.Contains(reason, "BridgeTargetProofCidMismatch") {
		t.Fatalf("expected BridgeTargetProofCidMismatch, got: %q", reason)
	}
}

// rejects_when_pinned_bundle_is_not_loaded (Rust analog).
//
// Pinned bundle isn't loaded at all: BundleMembers has no entry for
// it. Still a mismatch, fail-closed. The contract member IS present in
// the pool (this is what makes it the right test rather than a
// "target not in pool" test).
func TestResolveTarget_RejectsWhenPinnedBundleIsNotLoaded(t *testing.T) {
	targetCID := "blake3-512:contract-orphan"
	pool := poolWith(targetCID, contractEnv(trivialPre()))
	// Note: deliberately do NOT seed BundleMembers: the contract is
	// in the pool, but no bundle is registered for it.

	cs := CallSite{
		BridgeIRName:         "parseInt",
		BridgeTargetCID:      targetCID,
		BridgeTargetProofCID: "blake3-512:never-loaded",
	}

	stage := &ResolveTargetStage{}
	resolved, reason := stage.Run(cs, pool)
	if resolved != nil {
		t.Fatalf("must reject, got resolved=%+v reason=%q", resolved, reason)
	}
	if !strings.Contains(reason, "BridgeTargetProofCidMismatch") {
		t.Fatalf("expected BridgeTargetProofCidMismatch, got: %q", reason)
	}
}

// accepts_when_target_proof_cid_matches_bundle (Rust analog).
//
// Same bundle for the bridge and the contract member: accept and
// return the resolved formula.
func TestResolveTarget_AcceptsWhenTargetProofCidMatchesBundle(t *testing.T) {
	targetCID := "blake3-512:contract-pinned"
	honestBundle := "blake3-512:node-v24-proof-honest"

	pool := poolWith(targetCID, contractEnv(trivialPre()))
	pool.BundleMembers[honestBundle] = map[string]struct{}{
		targetCID: {},
	}

	cs := CallSite{
		BridgeIRName:         "parseInt",
		BridgeTargetCID:      targetCID,
		BridgeTargetProofCID: honestBundle,
	}

	stage := &ResolveTargetStage{}
	resolved, reason := stage.Run(cs, pool)
	if resolved == nil {
		t.Fatalf("must accept matching pin, got reason=%q", reason)
	}
	if resolved.CID != targetCID {
		t.Fatalf("resolved CID = %q, want %q", resolved.CID, targetCID)
	}
}

// accepts_when_target_proof_cid_is_none_back_compat (Rust analog).
//
// Legacy bridge with no `targetProofCid` (empty string is the Go
// analog of Rust's `None`): cannot enforce ConsequentBundlePinned,
// but accept for back-compat. resolve_target writes a soft warning to
// stderr; the test does not assert on stderr (matches Rust's pattern).
func TestResolveTarget_AcceptsWhenTargetProofCidIsAbsentBackCompat(t *testing.T) {
	targetCID := "blake3-512:contract-legacy"
	pool := poolWith(targetCID, contractEnv(trivialPre()))

	cs := CallSite{
		BridgeIRName:         "parseIntLegacy",
		BridgeTargetCID:      targetCID,
		BridgeTargetProofCID: "", // back-compat: no pin
	}

	stage := &ResolveTargetStage{}
	resolved, reason := stage.Run(cs, pool)
	if resolved == nil {
		t.Fatalf("legacy bridges must still resolve, got reason=%q", reason)
	}
	if resolved.CID != targetCID {
		t.Fatalf("resolved CID = %q, want %q", resolved.CID, targetCID)
	}
}
