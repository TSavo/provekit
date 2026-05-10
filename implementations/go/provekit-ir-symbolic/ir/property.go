package ir

import (
	"bytes"
	"fmt"
	"strings"
	"sync"
)

// Declaration is the kit-collected declaration tag. Contract and bridge
// are the v1.1.0 roles; the protocol cut dropped the standalone
// "property" declaration kind in favor of "contract" with optional
// pre/post/inv slots.
type Declaration interface {
	declMarker()
	Kind() string
	DeclName() string
}

// DefaultOutBinding is the conventional name a contract's post-formula
// uses to reference the function's return value. The kit's primitives
// don't enforce uniqueness; downstream verifiers treat it as the well-
// known free-variable name in the post slot.
const DefaultOutBinding = "out"

// EvidenceCertificate holds solver-specific proof data.
type EvidenceCertificate struct {
	Tool        string
	Version     string
	FormulaHash string
	ProofData   string
}

// EvidenceTerm attaches a proof certificate to a formula-bearing declaration.
type EvidenceTerm struct {
	ProofType   string // "smt-lib" | "coq" | "custom"
	Certificate EvidenceCertificate
}

func (e EvidenceTerm) MarshalJSON() ([]byte, error) {
	var buf bytes.Buffer
	buf.WriteString(`{"kind":"evidence","proofType":`)
	encoded, err := encodeJSON(e.ProofType)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteString(`,"certificate":{`)
	buf.WriteString(`"tool":`)
	encoded, err = encodeJSON(e.Certificate.Tool)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteString(`,"version":`)
	encoded, err = encodeJSON(e.Certificate.Version)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteString(`,"formulaHash":`)
	encoded, err = encodeJSON(e.Certificate.FormulaHash)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteString(`,"proofData":`)
	encoded, err = encodeJSON(e.Certificate.ProofData)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteString("}}")
	return buf.Bytes(), nil
}

// ContractDeclaration is the v1.1.0 replacement for PropertyDeclaration.
// Each of pre/post/inv is optional, but at least one MUST be non-nil
// (Contract panics otherwise). outBinding is the post-formula's
// return-value variable name; defaults to "out".
//
// JSON shape (locked key order: kind, name, outBinding, pre?, post?, inv?, evidence?):
//
//	{
//	  "kind": "contract",
//	  "name": "...",
//	  "outBinding": "out",
//	  "pre":  ...,
//	  "post": ...,
//	  "inv":  ...,
//	  "evidence": ...
//	}
//
// Empty/nil pre/post/inv are omitted entirely (JCS-friendly).
type ContractDeclaration struct {
	Name       string
	OutBinding string
	Pre        IrFormula
	Post       IrFormula
	Inv        IrFormula
	Evidence   *EvidenceTerm
}

func (ContractDeclaration) declMarker()        {}
func (ContractDeclaration) Kind() string       { return "contract" }
func (c ContractDeclaration) DeclName() string { return c.Name }

func (c ContractDeclaration) MarshalJSON() ([]byte, error) {
	var buf bytes.Buffer
	if c.Evidence != nil {
		buf.WriteString(`{"evidence":`)
		encoded, err := encodeJSON(c.Evidence)
		if err != nil {
			return nil, err
		}
		buf.Write(encoded)
		buf.WriteByte(',')
	} else {
		buf.WriteByte('{')
	}
	if c.Inv != nil {
		buf.WriteString(`"inv":`)
		encoded, err := encodeJSON(c.Inv)
		if err != nil {
			return nil, err
		}
		buf.Write(encoded)
		buf.WriteByte(',')
	}
	buf.WriteString(`"kind":"contract","name":`)
	encoded, err := encodeJSON(c.Name)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteString(`,"outBinding":`)
	encoded, err = encodeJSON(c.OutBinding)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	if c.Post != nil {
		buf.WriteString(`,"post":`)
		encoded, err = encodeJSON(c.Post)
		if err != nil {
			return nil, err
		}
		buf.Write(encoded)
	}
	if c.Pre != nil {
		buf.WriteString(`,"pre":`)
		encoded, err = encodeJSON(c.Pre)
		if err != nil {
			return nil, err
		}
		buf.Write(encoded)
	}
	buf.WriteByte('}')
	return buf.Bytes(), nil
}

// ContractArgs is the keyword-argument struct for Contract(). Each
// formula is optional; at least one of Pre/Post/Inv MUST be non-nil.
// OutBinding defaults to "out" when empty.
type ContractArgs struct {
	Pre        IrFormula
	Post       IrFormula
	Inv        IrFormula
	OutBinding string
	Evidence   *EvidenceTerm
}

// BridgeSpec is the input to Bridge(). The kit collector stores it as
// a BridgeDeclaration.
//
// SourceContractCid identifies the source-layer contract being bridged
// from. TargetProofCid is the CID of the .proof bundle containing the
// target contract; it makes cross-bundle lookup explicit and content-
// addressed (per protocol/specs/2026-04-30-ir-formal-grammar.md, the
// targetProofCid invariant promoted normative in PR #10).
type BridgeSpec struct {
	SourceSymbol      string
	SourceLayer       string
	SourceContractCid string
	TargetContractCid string
	TargetProofCid    string
	TargetLayer       string
	Notes             string
}

// BridgeDeclaration is the v1.1.0+ shape with sourceContractCid +
// targetProofCid pinning per the IR formal grammar. Locked key order
// (post PR #10): kind, name, sourceSymbol, sourceLayer,
// sourceContractCid, targetContractCid, targetProofCid, targetLayer,
// notes?: must be byte-equal across all four kits.
type BridgeDeclaration struct {
	Name              string
	SourceSymbol      string
	SourceLayer       string
	SourceContractCid string
	TargetContractCid string
	TargetProofCid    string
	TargetLayer       string
	Notes             string
}

func (BridgeDeclaration) declMarker()        {}
func (BridgeDeclaration) Kind() string       { return "bridge" }
func (b BridgeDeclaration) DeclName() string { return b.Name }

func (b BridgeDeclaration) MarshalJSON() ([]byte, error) {
	var buf bytes.Buffer
	buf.WriteString(`{"kind":"bridge","name":`)
	encoded, err := encodeJSON(b.Name)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteString(`,"sourceSymbol":`)
	encoded, err = encodeJSON(b.SourceSymbol)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteString(`,"sourceLayer":`)
	encoded, err = encodeJSON(b.SourceLayer)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteString(`,"sourceContractCid":`)
	encoded, err = encodeJSON(b.SourceContractCid)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteString(`,"targetContractCid":`)
	encoded, err = encodeJSON(b.TargetContractCid)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteString(`,"targetProofCid":`)
	encoded, err = encodeJSON(b.TargetProofCid)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteString(`,"targetLayer":`)
	encoded, err = encodeJSON(b.TargetLayer)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	if b.Notes != "" {
		buf.WriteString(`,"notes":`)
		encoded, err = encodeJSON(b.Notes)
		if err != nil {
			return nil, err
		}
		buf.Write(encoded)
	}
	buf.WriteByte('}')
	return buf.Bytes(), nil
}

// Locus identifies a source position for a call site.
// JSON shape (locked key order: column, file, line).
type Locus struct {
	File   string `json:"file"`
	Line   int    `json:"line"`
	Column int    `json:"column"`
}

func (l Locus) MarshalJSON() ([]byte, error) {
	var buf bytes.Buffer
	buf.WriteString(`{"column":`)
	encoded, err := encodeJSON(l.Column)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteString(`,"file":`)
	encoded, err = encodeJSON(l.File)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteString(`,"line":`)
	encoded, err = encodeJSON(l.Line)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteByte('}')
	return buf.Bytes(), nil
}

// CallEdgeDeclaration encodes a call site found by the lifter per
// protocol/specs/2026-05-03-bridge-linkage-protocol.md §1.
//
// JSON shape (JCS-canonical key order: callSiteLocus, evidenceTerm,
// kind, schemaVersion, sourceContractCid, targetContractCid,
// targetSymbol):
//
//	{
//	  "callSiteLocus":     { "column": N, "file": "...", "line": N },
//	  "evidenceTerm":      <IrFormula>,
//	  "kind":              "call-edge",
//	  "schemaVersion":     "1",
//	  "sourceContractCid": "blake3-512:...",
//	  "targetContractCid": "blake3-512:..." | null,
//	  "targetSymbol":      "..."
//	}
//
// targetContractCid is null for cross-kit calls (e.g. cgo); in that
// case targetSymbol carries the kit-prefixed symbol name (e.g.
// "rust-kit:rustFunc") for linker resolution per R3.
type CallEdgeDeclaration struct {
	SourceContractCid string
	TargetContractCid *string // nil encodes as JSON null
	TargetSymbol      string
	CallSiteLocus     Locus
	EvidenceTerm      IrFormula
}

func (CallEdgeDeclaration) declMarker()        {}
func (CallEdgeDeclaration) Kind() string       { return "call-edge" }
func (c CallEdgeDeclaration) DeclName() string { return c.SourceContractCid }

func (c CallEdgeDeclaration) MarshalJSON() ([]byte, error) {
	var buf bytes.Buffer
	buf.WriteString(`{"callSiteLocus":`)
	encoded, err := encodeJSON(c.CallSiteLocus)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteString(`,"evidenceTerm":`)
	encoded, err = encodeJSON(c.EvidenceTerm)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteString(`,"kind":"call-edge","schemaVersion":"1","sourceContractCid":`)
	encoded, err = encodeJSON(c.SourceContractCid)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteString(`,"targetContractCid":`)
	if c.TargetContractCid == nil {
		buf.WriteString("null")
	} else {
		encoded, err = encodeJSON(*c.TargetContractCid)
		if err != nil {
			return nil, err
		}
		buf.Write(encoded)
	}
	buf.WriteString(`,"targetSymbol":`)
	encoded, err = encodeJSON(c.TargetSymbol)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteByte('}')
	return buf.Bytes(), nil
}

// ----------------------------------------------------------------------
// Collector state
// ----------------------------------------------------------------------

var (
	collectorMu      sync.RWMutex
	activeCollector  *[]Declaration
	describePathSegs []string
)

// BeginCollecting starts a new collection scope. Returns a finalizer that
// retrieves the captured declarations and clears collector state. Not
// re-entrant; calling while another collection is active panics.
//
// Resets the quantifier counter so successive runs of the same invariant
// produce byte-identical IR.
func BeginCollecting() func() []Declaration {
	collectorMu.Lock()
	if activeCollector != nil {
		collectorMu.Unlock()
		panic("BeginCollecting: another collection is already active; lifting is not re-entrant")
	}
	collected := make([]Declaration, 0)
	activeCollector = &collected
	describePathSegs = nil
	collectorMu.Unlock()

	resetQuantifierCounter()

	return func() []Declaration {
		collectorMu.Lock()
		defer collectorMu.Unlock()
		out := *activeCollector
		activeCollector = nil
		describePathSegs = nil
		return out
	}
}

// ResetCollector clears any in-progress collector state. Use only in test
// setup/teardown to recover from leaked collectors.
func ResetCollector() {
	collectorMu.Lock()
	activeCollector = nil
	describePathSegs = nil
	collectorMu.Unlock()
	resetQuantifierCounter()
}

// Describe opens a named scope. Body runs immediately; Must / Contract
// calls inside register names as "<describe-path> > <name>". Nesting
// supported.
func Describe(name string, body func()) {
	collectorMu.Lock()
	describePathSegs = append(describePathSegs, name)
	collectorMu.Unlock()

	defer func() {
		collectorMu.Lock()
		if len(describePathSegs) > 0 {
			describePathSegs = describePathSegs[:len(describePathSegs)-1]
		}
		collectorMu.Unlock()
	}()

	body()
}

// DescribeSkip is a no-op; the body is not invoked.
func DescribeSkip(name string, body func()) {
	_ = name
	_ = body
}

// expandedName joins the active describe-path segments with the given
// leaf name using " > ".
func expandedName(name string) string {
	if len(describePathSegs) == 0 {
		return name
	}
	return strings.Join(describePathSegs, " > ") + " > " + name
}

// Contract registers a named contract with optional pre/post/inv slots.
// At least one of Pre/Post/Inv MUST be non-nil; panics otherwise.
// OutBinding defaults to "out".
//
// Active describe path prefixes the name (e.g. "Math > abs > non-negative").
func Contract(name string, args ContractArgs) {
	if args.Pre == nil && args.Post == nil && args.Inv == nil {
		panic(fmt.Sprintf(
			`Contract(%q, ...): at least one of Pre / Post / Inv must be non-nil`, name))
	}

	collectorMu.Lock()
	defer collectorMu.Unlock()

	if activeCollector == nil {
		panic(fmt.Sprintf(
			`Contract(%q, ...) called outside an active collector. Call BeginCollecting() first.`, name))
	}

	outBinding := args.OutBinding
	if outBinding == "" {
		outBinding = DefaultOutBinding
	}

	*activeCollector = append(*activeCollector, ContractDeclaration{
		Name:       expandedName(name),
		OutBinding: outBinding,
		Pre:        args.Pre,
		Post:       args.Post,
		Inv:        args.Inv,
		Evidence:   args.Evidence,
	})
}

// Must is the precondition-only alias for Contract. Mirrors the
// per-language-kit standard's must() primitive: must(name, formula) is
// shorthand for contract(name, ContractArgs{Pre: formula}).
func Must(name string, formula IrFormula) {
	Contract(name, ContractArgs{Pre: formula})
}

// MustSkip is a no-op; the formula is not collected.
func MustSkip(name string, formula IrFormula) {
	_ = name
	_ = formula
}

// Property is a back-compat alias; same shape as Must but skips
// describe-path expansion. Mirrors the TS kit's `property()` primitive.
//
// Deprecated: prefer Contract or Must. Retained so existing tests + lift
// adapters keep compiling during the v1.1.0 transition.
func Property(name string, formula IrFormula) {
	if formula == nil {
		panic(fmt.Sprintf(`Property(%q, ...): formula must be non-nil`, name))
	}

	collectorMu.Lock()
	defer collectorMu.Unlock()

	if activeCollector == nil {
		panic(fmt.Sprintf(
			`Property(%q, ...) called outside an active collector. Call BeginCollecting() first.`, name))
	}

	*activeCollector = append(*activeCollector, ContractDeclaration{
		Name:       name,
		OutBinding: DefaultOutBinding,
		Pre:        formula,
	})
}

// Bridge declares a host-language symbol bridges to a deeper-layer contract.
func Bridge(name string, spec BridgeSpec) {
	collectorMu.Lock()
	defer collectorMu.Unlock()

	if activeCollector == nil {
		panic(fmt.Sprintf(`Bridge(%q, ...) called outside an active collector. Call BeginCollecting() first.`, name))
	}

	*activeCollector = append(*activeCollector, BridgeDeclaration{
		Name:              name,
		SourceSymbol:      spec.SourceSymbol,
		SourceLayer:       spec.SourceLayer,
		SourceContractCid: spec.SourceContractCid,
		TargetContractCid: spec.TargetContractCid,
		TargetProofCid:    spec.TargetProofCid,
		TargetLayer:       spec.TargetLayer,
		Notes:             spec.Notes,
	})
}
