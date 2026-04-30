package claim_envelope

// PropertyMintArgs is the input to (*Minter).MintProperty.
//
// IRFormula and Scope are JSON-shape values (typically
// map[string]interface{} or wrappers around the IR types from the
// kit — the formula_to_value converter in the kit builds this shape).
type PropertyMintArgs struct {
	BindingHash  string
	PropertyHash string
	ProducedBy   string
	ProducedAt   string
	InputCIDs    []string
	IRFormula    interface{}
	Scope        interface{}
	IRKitVersion string
}

// MintProperty builds + signs a property ClaimEnvelope.
//
// The property memento is the load-bearing artifact bridges point at:
// resolving a bridge's targetContractCid yields one of these, and the
// embedded body.irFormula is the precondition the verifier instantiates
// at call sites.
func (m *Minter) MintProperty(args PropertyMintArgs) (*Minted, error) {
	body := map[string]interface{}{
		"irFormula":    args.IRFormula,
		"scope":        args.Scope,
		"irKitVersion": args.IRKitVersion,
	}
	evidence := map[string]interface{}{
		"kind":   "property",
		"schema": SchemaCIDProperty,
		"body":   body,
	}
	unsigned := envelopeForHashing(
		args.BindingHash, args.PropertyHash, VerdictHolds,
		args.ProducedBy, args.ProducedAt, args.InputCIDs, evidence,
	)
	return m.finalize(unsigned)
}
