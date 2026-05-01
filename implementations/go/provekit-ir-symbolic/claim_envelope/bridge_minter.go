package claim_envelope

import "fmt"

// BridgeMintArgs is the input to (*Minter).MintBridge.
//
// IRArgSorts: each element is a SortRef; either a primitive sort
// name as a string ("Int" / "Bool" / "String" / ...) or a sort object
// (map[string]interface{} with a kind discriminator).
//
// IRReturnSort: same shape as a SortRef.
//
// Notes: optional. Empty string → field omitted from the body.
//
// bindingHash and propertyHash are DERIVED per the v1.1.0 spec; callers
// MUST NOT supply them.
type BridgeMintArgs struct {
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

// MintBridge builds + signs a v1.1.0 bridge ClaimEnvelope.
//
// A bridge memento declares that a host-language symbol is the surface
// realization of a deeper-layer published contract. inputCids has
// exactly one entry equal to TargetContractCID (per the bridge-body
// REFERENT constraint in the memento envelope grammar).
//
// DERIVED per spec (v1.1.0; full BLAKE3-512 with "blake3-512:" prefix):
//
//	bindingHash  = ComputeCID(canonical({sourceLayer, sourceSymbol}))
//	propertyHash = ComputeCID("bridge:" || sourceSymbol)   (raw string, NOT JCS-wrapped)
func (m *Minter) MintBridge(args BridgeMintArgs) (*Minted, error) {
	if args.SourceSymbol == "" {
		return nil, fmt.Errorf("MintBridge: SourceSymbol is required")
	}
	if args.SourceLayer == "" {
		return nil, fmt.Errorf("MintBridge: SourceLayer is required")
	}
	if args.TargetContractCID == "" {
		return nil, fmt.Errorf("MintBridge: TargetContractCID is required")
	}

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

	bindingHash, err := hashValue(map[string]interface{}{
		"sourceLayer":  args.SourceLayer,
		"sourceSymbol": args.SourceSymbol,
	})
	if err != nil {
		return nil, fmt.Errorf("MintBridge: bindingHash: %w", err)
	}
	propertyHash := hashRawString("bridge:" + args.SourceSymbol)

	unsigned := envelopeForHashing(
		bindingHash, propertyHash, VerdictHolds,
		args.ProducedBy, args.ProducedAt,
		[]string{args.TargetContractCID},
		evidence,
	)
	return m.finalize(unsigned)
}
