package claim_envelope

import (
	"fmt"
)

// AuthoringKind identifies which producer-kind variant of the
// contract memento's authoring block applies.
type AuthoringKind int

const (
	AuthoringKitAuthor AuthoringKind = iota + 1
	AuthoringLift
	AuthoringLLM
)

// AuthoringKitAuthorArgs is the producer-kind="kit-author" variant.
type AuthoringKitAuthorArgs struct {
	Author string // producer-id, e.g. "go-kit@1.0"
	Note   string // optional; "" → omitted
}

// AuthoringLiftArgs is the producer-kind="lift" variant.
type AuthoringLiftArgs struct {
	Lifter    string // producer-id, e.g. "provekit-lift@1.0"
	Evidence  string // "tests" | "types" | "docs" | "symbolic-exec"
	SourceCid string // optional; "" → omitted
}

// AuthoringLLMArgs is the producer-kind="llm" variant.
type AuthoringLLMArgs struct {
	LLM        string
	LLMVersion string
	PromptCid  string
	Confidence float64
	Rationale  string // optional; "" → omitted
}

// ContractMintArgs is the input to (*Minter).MintContract.
//
// Each of Pre / Post / Inv is optional, but at least one MUST be
// non-nil; MintContract returns an error otherwise. OutBinding is
// required and is conventionally "out".
//
// Pre / Post / Inv are JSON-shape values (typically map[string]any
// or wrappers around the IR types; use FormulaToValue to convert
// from a kit IrFormula).
//
// preHash / postHash / invHash, propertyHash and bindingHash are all
// DERIVED by this minter; callers MUST NOT supply them.
type ContractMintArgs struct {
	ContractName string
	Pre          interface{}
	Post         interface{}
	Inv          interface{}
	OutBinding   string
	ProducedBy   string
	ProducedAt   string
	InputCIDs    []string

	AuthoringKind       AuthoringKind
	AuthoringKitAuthor  AuthoringKitAuthorArgs
	AuthoringLift       AuthoringLiftArgs
	AuthoringLLM        AuthoringLLMArgs
}

// MintContract builds + signs a v1.1.0 contract ClaimEnvelope.
//
// Body shape per protocol/specs/2026-04-30-memento-envelope-grammar.md
// (Role: ContractMemento). At mint time the minter:
//
//  1. computes preHash / postHash / invHash from each present formula
//     (hash16 of JCS-canonical bytes);
//  2. computes propertyHash = hash16(canonical({pre?, post?, inv?, outBinding}));
//  3. computes bindingHash  = hash16(canonical({producerId, contractName, propertyHash}));
//  4. assembles + signs the envelope.
//
// The protocol cut is scorched-earth: callers no longer pass any hash
// in. Any caller-supplied hash is a bug.
func (m *Minter) MintContract(args ContractMintArgs) (*Minted, error) {
	if args.Pre == nil && args.Post == nil && args.Inv == nil {
		return nil, fmt.Errorf("MintContract: at least one of Pre/Post/Inv must be non-nil")
	}
	if args.OutBinding == "" {
		return nil, fmt.Errorf("MintContract: OutBinding is required")
	}
	if args.ContractName == "" {
		return nil, fmt.Errorf("MintContract: ContractName is required")
	}

	// Build the contract body. Locked field set:
	//   contractName, outBinding, pre?, preHash?, post?, postHash?, inv?, invHash?, authoring
	body := map[string]interface{}{
		"contractName": args.ContractName,
		"outBinding":   args.OutBinding,
	}
	if args.Pre != nil {
		body["pre"] = args.Pre
		ph, err := hash16Value(args.Pre)
		if err != nil {
			return nil, fmt.Errorf("MintContract: pre hash16: %w", err)
		}
		body["preHash"] = ph
	}
	if args.Post != nil {
		body["post"] = args.Post
		ph, err := hash16Value(args.Post)
		if err != nil {
			return nil, fmt.Errorf("MintContract: post hash16: %w", err)
		}
		body["postHash"] = ph
	}
	if args.Inv != nil {
		body["inv"] = args.Inv
		ih, err := hash16Value(args.Inv)
		if err != nil {
			return nil, fmt.Errorf("MintContract: inv hash16: %w", err)
		}
		body["invHash"] = ih
	}
	authoring, err := buildAuthoring(args)
	if err != nil {
		return nil, err
	}
	body["authoring"] = authoring

	evidence := map[string]interface{}{
		"kind":   "contract",
		"schema": SchemaCIDContract,
		"body":   body,
	}

	// DERIVED:
	//   propertyHash = hash16(canonical({pre?, post?, inv?, outBinding}))
	phObj := map[string]interface{}{
		"outBinding": args.OutBinding,
	}
	if args.Pre != nil {
		phObj["pre"] = args.Pre
	}
	if args.Post != nil {
		phObj["post"] = args.Post
	}
	if args.Inv != nil {
		phObj["inv"] = args.Inv
	}
	propertyHash, err := hash16Value(phObj)
	if err != nil {
		return nil, fmt.Errorf("MintContract: propertyHash: %w", err)
	}

	// DERIVED:
	//   bindingHash = hash16(canonical({producerId, contractName, propertyHash}))
	bhObj := map[string]interface{}{
		"producerId":   args.ProducedBy,
		"contractName": args.ContractName,
		"propertyHash": propertyHash,
	}
	bindingHash, err := hash16Value(bhObj)
	if err != nil {
		return nil, fmt.Errorf("MintContract: bindingHash: %w", err)
	}

	unsigned := envelopeForHashing(
		bindingHash, propertyHash, VerdictHolds,
		args.ProducedBy, args.ProducedAt, args.InputCIDs, evidence,
	)
	return m.finalize(unsigned)
}

// buildAuthoring renders the typed-union authoring block. Each variant
// has producerKind as the discriminator.
func buildAuthoring(args ContractMintArgs) (map[string]interface{}, error) {
	switch args.AuthoringKind {
	case AuthoringKitAuthor:
		out := map[string]interface{}{
			"producerKind": "kit-author",
			"author":       args.AuthoringKitAuthor.Author,
		}
		if args.AuthoringKitAuthor.Note != "" {
			out["note"] = args.AuthoringKitAuthor.Note
		}
		return out, nil
	case AuthoringLift:
		out := map[string]interface{}{
			"producerKind": "lift",
			"lifter":       args.AuthoringLift.Lifter,
			"evidence":     args.AuthoringLift.Evidence,
		}
		if args.AuthoringLift.SourceCid != "" {
			out["sourceCid"] = args.AuthoringLift.SourceCid
		}
		return out, nil
	case AuthoringLLM:
		out := map[string]interface{}{
			"producerKind": "llm",
			"llm":          args.AuthoringLLM.LLM,
			"llmVersion":   args.AuthoringLLM.LLMVersion,
			"promptCid":    args.AuthoringLLM.PromptCid,
			// Encoded losslessly as integer permille for v0; see C++ ref.
			"confidence": int64(args.AuthoringLLM.Confidence * 1000),
		}
		if args.AuthoringLLM.Rationale != "" {
			out["rationale"] = args.AuthoringLLM.Rationale
		}
		return out, nil
	default:
		return nil, fmt.Errorf("MintContract: unknown authoring kind %d", args.AuthoringKind)
	}
}
