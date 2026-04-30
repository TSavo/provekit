// Package ir extension authoring + primitive-bridge registry.
//
// Mirrors the TS kit's src/ir/extensions/ and the Rust kit's src/extensions.rs.
// Per the IR extension protocol (docs/specs/2026-04-30-ir-extension-protocol.md),
// kit authors declare new sorts/predicates/ctors as extensions OR bridge
// to deeper-layer authorities (V8 in TS, Rust core in Rust, Go runtime
// in Go) via the registry's two factory shapes.
//
// extensionSort / extensionPredicate / extensionCtor — kit OWNS the
//   semantics. For kit-idiomatic primitives or user-authored extensions.
//
// PrimitiveBridge — kit REFERENCES a deeper layer. The kit emits IR ctor
//   nodes referencing the bridged name; the verifier resolves through
//   the protocol to the deeper layer's signed declaration.

package ir

import (
	"encoding/json"
	"fmt"
	"sync"
)

// SemanticDeclaration variants. Tagged union via the Kind field.
type SemanticDeclaration struct {
	Kind        string          `json:"kind"`
	Theory      string          `json:"theory,omitempty"`
	Version     string          `json:"version,omitempty"`
	Axioms      json.RawMessage `json:"axioms,omitempty"`
	System      string          `json:"system,omitempty"`
	Identifier  string          `json:"identifier,omitempty"`
	ProofCID    string          `json:"proofCid,omitempty"`
	Text        string          `json:"text,omitempty"`
}

// SortRef is either a name (string) or a Sort value. Encoded as either a
// JSON string or a JSON object.
type SortRef struct {
	Named string `json:"-"`
	Sort  Sort   `json:"-"`
}

func (s SortRef) MarshalJSON() ([]byte, error) {
	if s.Named != "" {
		return json.Marshal(s.Named)
	}
	return json.Marshal(s.Sort)
}

func (s SortRef) toSort() Sort {
	if s.Named != "" {
		return primitiveSort{Name: s.Named}
	}
	return s.Sort
}

// SortParam describes one parameter of a parameterized sort declaration.
type SortParam struct {
	Name      string `json:"name"`
	ParamSort string `json:"paramSort"`
}

// ExtensionDeclaration is the protocol-shape memento body. The Introduces
// field discriminates sort / predicate / ctor; the relevant fields are
// populated based on that.
type ExtensionDeclaration struct {
	Introduces string                `json:"introduces"`
	Name       string                `json:"name"`
	Params     []SortParam           `json:"params,omitempty"`
	ArgSorts   []SortRef             `json:"argSorts,omitempty"`
	ReturnSort *SortRef              `json:"returnSort,omitempty"`
	Semantics  []SemanticDeclaration `json:"semantics"`
	Compilers  []string              `json:"compilers"`
}

// PrimitiveBridgeDeclaration is the bridge-memento body for a kit
// primitive that references a deeper-layer authority.
type PrimitiveBridgeDeclaration struct {
	IRName            string    `json:"irName"`
	IRArgSorts        []SortRef `json:"irArgSorts"`
	IRReturnSort      SortRef   `json:"irReturnSort"`
	SourceLayer       string    `json:"sourceLayer"`
	TargetContractCID string    `json:"targetContractCid"`
	TargetLayer       string    `json:"targetLayer"`
	Notes             string    `json:"notes,omitempty"`
}

// Registry state. Process-global, mutex-protected.
var (
	registryMu  sync.Mutex
	extensions  = map[string]ExtensionDeclaration{}
	bridges     = map[string]PrimitiveBridgeDeclaration{}
)

// RegisterExtensionDeclaration registers a sort/predicate/ctor extension.
// Idempotent for byte-identical re-registration; returns an error on
// collision with a different body.
func RegisterExtensionDeclaration(decl ExtensionDeclaration) error {
	registryMu.Lock()
	defer registryMu.Unlock()
	if existing, ok := extensions[decl.Name]; ok {
		if !sameExtension(existing, decl) {
			return fmt.Errorf("extension %q already registered with a different declaration", decl.Name)
		}
		return nil
	}
	extensions[decl.Name] = decl
	return nil
}

// RegisterPrimitiveBridge registers a kit primitive's bridge to a deeper
// layer. Idempotent for byte-identical re-registration; returns an error
// on collision with a different target.
func RegisterPrimitiveBridge(decl PrimitiveBridgeDeclaration) error {
	registryMu.Lock()
	defer registryMu.Unlock()
	if existing, ok := bridges[decl.IRName]; ok {
		if !sameBridge(existing, decl) {
			return fmt.Errorf("primitive bridge %q already registered with a different target", decl.IRName)
		}
		return nil
	}
	bridges[decl.IRName] = decl
	return nil
}

// LookupExtension returns the declaration registered under name, or
// nil when no extension has that name.
func LookupExtension(name string) *ExtensionDeclaration {
	registryMu.Lock()
	defer registryMu.Unlock()
	if decl, ok := extensions[name]; ok {
		return &decl
	}
	return nil
}

// LookupBridge returns the bridge declaration registered under irName.
func LookupBridge(irName string) *PrimitiveBridgeDeclaration {
	registryMu.Lock()
	defer registryMu.Unlock()
	if decl, ok := bridges[irName]; ok {
		return &decl
	}
	return nil
}

// ListExtensions returns all registered extension declarations.
func ListExtensions() []ExtensionDeclaration {
	registryMu.Lock()
	defer registryMu.Unlock()
	out := make([]ExtensionDeclaration, 0, len(extensions))
	for _, v := range extensions {
		out = append(out, v)
	}
	return out
}

// ListBridges returns all registered primitive-bridge declarations.
func ListBridges() []PrimitiveBridgeDeclaration {
	registryMu.Lock()
	defer registryMu.Unlock()
	out := make([]PrimitiveBridgeDeclaration, 0, len(bridges))
	for _, v := range bridges {
		out = append(out, v)
	}
	return out
}

// ResetRegistry clears extension + bridge state. Tests use this to
// isolate cases. Also resets the kit-bridges-registered Once so
// subsequent calls to bridged primitives re-trigger lazy init.
func ResetRegistry() {
	registryMu.Lock()
	defer registryMu.Unlock()
	extensions = map[string]ExtensionDeclaration{}
	bridges = map[string]PrimitiveBridgeDeclaration{}
	kitBridgesOnce = sync.Once{}
}

// ExtensionSort declares a new sort and returns its Sort value.
func ExtensionSort(name string, params []SortParam, semantics []SemanticDeclaration, compilers []string) Sort {
	decl := ExtensionDeclaration{
		Introduces: "sort",
		Name:       name,
		Params:     params,
		Semantics:  semantics,
		Compilers:  compilers,
	}
	if err := RegisterExtensionDeclaration(decl); err != nil {
		panic(fmt.Sprintf("ExtensionSort: %v", err))
	}
	return primitiveSort{Name: name}
}

// ExtensionPredicate declares a new predicate and returns a builder
// that constructs atomic IrFormulas.
func ExtensionPredicate(name string, argSorts []SortRef, semantics []SemanticDeclaration, compilers []string) func(args ...IrTerm) IrFormula {
	decl := ExtensionDeclaration{
		Introduces: "predicate",
		Name:       name,
		ArgSorts:   argSorts,
		Semantics:  semantics,
		Compilers:  compilers,
	}
	if err := RegisterExtensionDeclaration(decl); err != nil {
		panic(fmt.Sprintf("ExtensionPredicate: %v", err))
	}
	return func(args ...IrTerm) IrFormula {
		return atom(name, args)
	}
}

// ExtensionCtor declares a new term constructor and returns a builder.
func ExtensionCtor(name string, argSorts []SortRef, returnSort SortRef, semantics []SemanticDeclaration, compilers []string) func(args ...IrTerm) IrTerm {
	rs := returnSort
	decl := ExtensionDeclaration{
		Introduces: "ctor",
		Name:       name,
		ArgSorts:   argSorts,
		ReturnSort: &rs,
		Semantics:  semantics,
		Compilers:  compilers,
	}
	if err := RegisterExtensionDeclaration(decl); err != nil {
		panic(fmt.Sprintf("ExtensionCtor: %v", err))
	}
	resolved := returnSort.toSort()
	return func(args ...IrTerm) IrTerm {
		return ctor(name, args, resolved)
	}
}

// PrimitiveBridge declares a kit-references-not-owns primitive and
// returns a builder.
func PrimitiveBridge(irName string, irArgSorts []SortRef, irReturnSort SortRef, sourceLayer, targetCID, targetLayer string, notes string) func(args ...IrTerm) IrTerm {
	decl := PrimitiveBridgeDeclaration{
		IRName:            irName,
		IRArgSorts:        irArgSorts,
		IRReturnSort:      irReturnSort,
		SourceLayer:       sourceLayer,
		TargetContractCID: targetCID,
		TargetLayer:       targetLayer,
		Notes:             notes,
	}
	if err := RegisterPrimitiveBridge(decl); err != nil {
		panic(fmt.Sprintf("PrimitiveBridge: %v", err))
	}
	resolved := irReturnSort.toSort()
	return func(args ...IrTerm) IrTerm {
		return ctor(irName, args, resolved)
	}
}

// -- helpers --

func sameExtension(a, b ExtensionDeclaration) bool {
	ja, _ := json.Marshal(a)
	jb, _ := json.Marshal(b)
	return string(ja) == string(jb)
}

func sameBridge(a, b PrimitiveBridgeDeclaration) bool {
	ja, _ := json.Marshal(a)
	jb, _ := json.Marshal(b)
	return string(ja) == string(jb)
}

// -- Lazy registration of kit bridges --
//
// On first call to any bridged primitive in primitives.go, ensureKitBridgesRegistered
// populates the registry. Same pattern as the TS kit's module-load
// registration and the Rust kit's OnceLock-gated registration.

var kitBridgesOnce sync.Once

func ensureKitBridgesRegistered() {
	kitBridgesOnce.Do(func() {
		bridgesToRegister := []struct {
			IRName     string
			ArgSorts   []SortRef
			ReturnSort SortRef
			TargetCID  string
		}{
			{"parseInt", []SortRef{{Named: "String"}}, SortRef{Named: "Int"}, "bafy_GO_PARSEINT_PLACEHOLDER"},
			{"parseFloat", []SortRef{{Named: "String"}}, SortRef{Named: "Real"}, "bafy_GO_PARSEFLOAT_PLACEHOLDER"},
			{"isNaN", []SortRef{{Named: "Real"}}, SortRef{Named: "Bool"}, "bafy_GO_ISNAN_PLACEHOLDER"},
			{"isFinite", []SortRef{{Named: "Real"}}, SortRef{Named: "Bool"}, "bafy_GO_ISFINITE_PLACEHOLDER"},
			{"isInteger", []SortRef{{Named: "Real"}}, SortRef{Named: "Bool"}, "bafy_GO_ISINTEGER_PLACEHOLDER"},
			{"Math.floor", []SortRef{{Named: "Real"}}, SortRef{Named: "Int"}, "bafy_GO_FLOOR_PLACEHOLDER"},
			{"Math.ceil", []SortRef{{Named: "Real"}}, SortRef{Named: "Int"}, "bafy_GO_CEIL_PLACEHOLDER"},
			{"Math.sqrt", []SortRef{{Named: "Real"}}, SortRef{Named: "Real"}, "bafy_GO_SQRT_PLACEHOLDER"},
			{"Math.sign", []SortRef{{Named: "Real"}}, SortRef{Named: "Int"}, "bafy_GO_SIGN_PLACEHOLDER"},
			{"String.prototype.length", []SortRef{{Named: "String"}}, SortRef{Named: "Int"}, "bafy_GO_STRLEN_PLACEHOLDER"},
			{"String.prototype.includes", []SortRef{{Named: "String"}, {Named: "String"}}, SortRef{Named: "Bool"}, "bafy_GO_STRINCLUDES_PLACEHOLDER"},
			{"Array.prototype.length", []SortRef{{Named: "Array"}}, SortRef{Named: "Int"}, "bafy_GO_ARRLEN_PLACEHOLDER"},
			{"Array.prototype.includes", []SortRef{{Named: "Array"}, {Named: "Any"}}, SortRef{Named: "Bool"}, "bafy_GO_ARRINCLUDES_PLACEHOLDER"},
		}
		for _, b := range bridgesToRegister {
			_ = RegisterPrimitiveBridge(PrimitiveBridgeDeclaration{
				IRName:            b.IRName,
				IRArgSorts:        b.ArgSorts,
				IRReturnSort:      b.ReturnSort,
				SourceLayer:       "go-kit",
				TargetContractCID: b.TargetCID,
				TargetLayer:       "go-runtime",
			})
		}
	})
}
