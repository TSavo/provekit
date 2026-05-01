package verifier

// ResolveTargetStage hash-looks-up a bridge's targetContractCid in the
// pool and returns the resolved contract memento's `pre` formula. O(1):
// no file IO; pool was built once at LoadAllProofsStage.
//
// v1.1.0 cut: the bridge target is a CONTRACT memento; the consumer-side
// discharge targets the contract's `pre` slot (the precondition the
// caller must establish at the call site). post/inv participate in the
// handshake algorithm via their own slots.
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
	pre, ok := body["pre"]
	if !ok {
		return nil, "no-pre-slot"
	}
	return &ResolvedProperty{
		CID:       cs.BridgeTargetCID,
		IRFormula: pre,
	}, ""
}
