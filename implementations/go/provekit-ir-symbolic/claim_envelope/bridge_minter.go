package claim_envelope

// BridgeMintArgs is the input to (*Minter).MintBridge.
//
// IRArgSorts: each element is a SortRef — either a primitive sort
// name as a string ("Int" / "Bool" / "String" / ...) or a sort object
// (map[string]interface{} with a kind discriminator).
//
// IRReturnSort: same shape as a SortRef.
//
// Notes: optional. Empty string → field omitted from the body.
type BridgeMintArgs struct {
	BindingHash       string
	PropertyHash      string
	ProducedBy        string
	ProducedAt        string
	SourceSymbol      string
	SourceLayer       string
	TargetContractCID string
	TargetLayer       string
	IRArgSorts        []interface{}
	IRReturnSort      interface{}
	Notes             string
}

// MintBridge builds + signs a bridge ClaimEnvelope.
//
// A bridge memento declares that a host-language symbol is the surface
// realization of a deeper-layer published contract. inputCids has
// exactly one entry equal to TargetContractCID (per the bridge-body
// REFERENT constraint in the memento envelope grammar).
func (m *Minter) MintBridge(args BridgeMintArgs) (*Minted, error) {
	body := map[string]interface{}{
		"sourceSymbol":      args.SourceSymbol,
		"sourceLayer":       args.SourceLayer,
		"targetContractCid": args.TargetContractCID,
		"targetLayer":       args.TargetLayer,
		"irArgSorts":        args.IRArgSorts,
		"irReturnSort":      args.IRReturnSort,
	}
	if args.Notes != "" {
		body["notes"] = args.Notes
	}
	evidence := map[string]interface{}{
		"kind":   "bridge",
		"schema": SchemaCIDBridge,
		"body":   body,
	}
	unsigned := envelopeForHashing(
		args.BindingHash, args.PropertyHash, VerdictHolds,
		args.ProducedBy, args.ProducedAt,
		[]string{args.TargetContractCID},
		evidence,
	)
	return m.finalize(unsigned)
}
