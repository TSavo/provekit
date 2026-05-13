package claim_envelope

import (
	"crypto/ed25519"
	"encoding/base64"
	"fmt"
)

// BridgeTargetV14 is the tagged-union target per
// 2026-05-03-bridge-target-dimensionality.md §1.R1.
type BridgeTargetV14 struct {
	Kind string // "contract" or "contractSet"
	CID  string
}

// BridgeMintV14Args is the input to (*Minter).MintBridgeV14.
//
// Metadata fields with empty strings are OMITTED from JCS bytes
// per spec §1.R2 (no null, no placeholder strings).
type BridgeMintV14Args struct {
	Name              string
	SourceSymbol      string
	SourceLayer       string
	SourceContractCID string
	Target            BridgeTargetV14

	// metadata (optional, empty = omit)
	TargetWitnessCID     string
	TargetBinaryCID      string
	TargetLayer          string
	TargetContractSetCID string
	ProducedBy           string
	ProducedAt           string

	DeclaredAt string
}

// MintBridgeV14 builds + signs a v1.4 layered bridge claim envelope
// ({envelope, header, metadata}) with tagged-union target.
//
// Canonical reference: rust/provekit-claim-envelope/src/lib.rs fn mint_bridge_v14.
func (m *Minter) MintBridgeV14(args BridgeMintV14Args) (*Minted, error) {
	if args.Name == "" {
		return nil, fmt.Errorf("MintBridgeV14: Name is required")
	}
	if args.Target.Kind != "contract" && args.Target.Kind != "contractSet" {
		return nil, fmt.Errorf("MintBridgeV14: Target.Kind must be 'contract' or 'contractSet'")
	}

	// Build target
	target := map[string]interface{}{
		"kind": args.Target.Kind,
		"cid":  args.Target.CID,
	}

	// Build header (7 canonical fields per spec §1.R3)
	header := map[string]interface{}{
		"schemaVersion":     "1",
		"kind":              "bridge",
		"name":              args.Name,
		"sourceSymbol":      args.SourceSymbol,
		"sourceLayer":       args.SourceLayer,
		"sourceContractCid": args.SourceContractCID,
		"target":            target,
	}

	// Build metadata (only non-empty fields per §1.R2)
	meta := map[string]interface{}{}
	emitIfNotEmpty := func(key, val string) {
		if val != "" {
			meta[key] = val
		}
	}
	emitIfNotEmpty("targetWitnessCid", args.TargetWitnessCID)
	emitIfNotEmpty("targetBinaryCid", args.TargetBinaryCID)
	emitIfNotEmpty("targetLayer", args.TargetLayer)
	emitIfNotEmpty("targetContractSetCid", args.TargetContractSetCID)
	emitIfNotEmpty("producedBy", args.ProducedBy)
	emitIfNotEmpty("producedAt", args.ProducedAt)

	// Sign: JCS({header, metadata})
	sigPayloadObj := map[string]interface{}{
		"header":   header,
		"metadata": meta,
	}
	sigPayloadJCS, err := m.encoder.Encode(sigPayloadObj)
	if err != nil {
		return nil, fmt.Errorf("MintBridgeV14: encode sig payload: %w", err)
	}
	sig := ed25519.Sign(m.signer, sigPayloadJCS)
	sigStr := Ed25519SigPrefix + base64.StdEncoding.EncodeToString(sig)

	// Build envelope with signature
	pubkey := base64.StdEncoding.EncodeToString(m.signer.Public().(ed25519.PublicKey))
	env := map[string]interface{}{
		"signer":     Ed25519SigPrefix + pubkey,
		"declaredAt": args.DeclaredAt,
		"signature":  sigStr,
	}

	// Full memento: {envelope, header, metadata}
	memento := map[string]interface{}{
		"envelope": env,
		"header":   header,
		"metadata": meta,
	}
	canonical, err := m.encoder.Encode(memento)
	if err != nil {
		return nil, fmt.Errorf("MintBridgeV14: encode memento: %w", err)
	}
	cid := m.hasher.ComputeCID(canonical)

	return &Minted{
		CanonicalBytes: canonical,
		CID:            cid,
	}, nil
}
