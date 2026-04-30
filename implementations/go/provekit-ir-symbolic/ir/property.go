package ir

import (
	"bytes"
	"fmt"
	"strings"
	"sync"
)

type Declaration interface {
	declMarker()
	Kind() string
	DeclName() string
}

type PropertyDeclaration struct {
	Name    string
	Formula IrFormula
}

func (PropertyDeclaration) declMarker()       {}
func (PropertyDeclaration) Kind() string      { return "property" }
func (p PropertyDeclaration) DeclName() string { return p.Name }

func (p PropertyDeclaration) MarshalJSON() ([]byte, error) {
	var buf bytes.Buffer
	buf.WriteString(`{"kind":"property","name":`)
	encoded, err := encodeJSON(p.Name)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteString(`,"formula":`)
	encoded, err = encodeJSON(p.Formula)
	if err != nil {
		return nil, err
	}
	buf.Write(encoded)
	buf.WriteByte('}')
	return buf.Bytes(), nil
}

type BridgeSpec struct {
	SourceSymbol      string
	SourceLayer       string
	TargetContractCid string
	TargetLayer       string
	Notes             string
}

type BridgeDeclaration struct {
	Name              string
	SourceSymbol      string
	SourceLayer       string
	TargetContractCid string
	TargetLayer       string
	Notes             string
}

func (BridgeDeclaration) declMarker()       {}
func (BridgeDeclaration) Kind() string      { return "bridge" }
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
// produce byte-identical IR (matches TS kit behavior).
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

// Describe opens a named scope. Body runs immediately; Must calls inside
// register names as "<describe-path> > <name>". Nesting supported.
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

// Must registers a named invariant. Active describe path prefixes the name.
func Must(name string, formula IrFormula) {
	collectorMu.Lock()
	defer collectorMu.Unlock()

	if activeCollector == nil {
		panic(fmt.Sprintf(`Must(%q, ...) called outside an active collector. Call BeginCollecting() first.`, name))
	}

	fullName := name
	if len(describePathSegs) > 0 {
		fullName = strings.Join(describePathSegs, " > ") + " > " + name
	}

	*activeCollector = append(*activeCollector, PropertyDeclaration{
		Name:    fullName,
		Formula: formula,
	})
}

// MustSkip is a no-op; the formula is not collected.
func MustSkip(name string, formula IrFormula) {
	_ = name
	_ = formula
}

// Property registers a property with an explicit name (no describe-path
// prefixing). Mirrors the TS `property()` primitive.
func Property(name string, formula IrFormula) {
	collectorMu.Lock()
	defer collectorMu.Unlock()

	if activeCollector == nil {
		panic(fmt.Sprintf(`Property(%q, ...) called outside an active collector. Call BeginCollecting() first.`, name))
	}

	*activeCollector = append(*activeCollector, PropertyDeclaration{
		Name:    name,
		Formula: formula,
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
