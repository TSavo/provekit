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

// ContractDeclaration is the v1.1.0 replacement for PropertyDeclaration.
// Each of pre/post/inv is optional, but at least one MUST be non-nil
// (Contract panics otherwise). outBinding is the post-formula's
// return-value variable name; defaults to "out".
//
// JSON shape (locked key order: kind, name, outBinding, pre?, post?, inv?):
//
//	{
//	  "kind": "contract",
//	  "name": "...",
//	  "outBinding": "out",
//	  "pre":  ...,
//	  "post": ...,
//	  "inv":  ...
//	}
//
// Empty/nil pre/post/inv are omitted entirely (JCS-friendly).
type ContractDeclaration struct {
	Name       string
	OutBinding string
	Pre        IrFormula
	Post       IrFormula
	Inv        IrFormula
}

func (ContractDeclaration) declMarker()        {}
func (ContractDeclaration) Kind() string       { return "contract" }
func (c ContractDeclaration) DeclName() string { return c.Name }

func (c ContractDeclaration) MarshalJSON() ([]byte, error) {
	var buf bytes.Buffer
	buf.WriteString(`{"kind":"contract","name":`)
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
	if c.Pre != nil {
		buf.WriteString(`,"pre":`)
		encoded, err = encodeJSON(c.Pre)
		if err != nil {
			return nil, err
		}
		buf.Write(encoded)
	}
	if c.Post != nil {
		buf.WriteString(`,"post":`)
		encoded, err = encodeJSON(c.Post)
		if err != nil {
			return nil, err
		}
		buf.Write(encoded)
	}
	if c.Inv != nil {
		buf.WriteString(`,"inv":`)
		encoded, err = encodeJSON(c.Inv)
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
}

// BridgeSpec is the input to Bridge(). The kit collector stores it as
// a BridgeDeclaration.
type BridgeSpec struct {
	SourceSymbol      string
	SourceLayer       string
	TargetContractCid string
	TargetLayer       string
	Notes             string
}

// BridgeDeclaration is unchanged across the v1.1.0 cut — bridges still
// declare host-language symbol → contract-memento CID linkage.
type BridgeDeclaration struct {
	Name              string
	SourceSymbol      string
	SourceLayer       string
	TargetContractCid string
	TargetLayer       string
	Notes             string
}

func (BridgeDeclaration) declMarker()         {}
func (BridgeDeclaration) Kind() string        { return "bridge" }
func (b BridgeDeclaration) DeclName() string  { return b.Name }

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
	buf.WriteString(`,"targetContractCid":`)
	encoded, err = encodeJSON(b.TargetContractCid)
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
// re-entrant — calling while another collection is active panics.
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

// Property is a back-compat alias — same shape as Must but skips
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
		TargetContractCid: spec.TargetContractCid,
		TargetLayer:       spec.TargetLayer,
		Notes:             spec.Notes,
	})
}
