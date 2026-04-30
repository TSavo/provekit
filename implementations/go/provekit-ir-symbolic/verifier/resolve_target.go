package verifier

// ResolveTargetStage hash-looks-up a bridge's targetContractCid in the
// pool and returns the resolved property memento's IR formula. O(1)
// — no file IO; pool was built once at LoadAllProofsStage.
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
	if ev["kind"] != "property" {
		return nil, "not-property-variant"
	}
	body, _ := ev["body"].(map[string]interface{})
	if body == nil {
		return nil, "body-missing"
	}
	return &ResolvedProperty{
		CID:          cs.BridgeTargetCID,
		IRFormula:    body["irFormula"],
		Scope:        body["scope"],
		IRKitVersion: asString(body["irKitVersion"]),
	}, ""
}
