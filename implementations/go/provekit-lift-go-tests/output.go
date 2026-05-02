// Package lifgotests implements the ProvekIt Layer 2 lift adapter for
// Go test files. It walks `*_test.go` source via go/parser + go/ast and
// emits canonical IR mementos (ContractDeclaration values) for three
// structural patterns Layer 0 cannot directly recognize:
//
//   - Pattern 1: a bounded for-loop as a universal quantifier
//     forall i: Int. (lo <= i AND i </<= hi) implies <body assertion>.
//   - Pattern 2: helper-function inlining; one memento per call site.
//   - Pattern 3: multi-assertion characterization conjunction; ≥2
//     liftable top-level assertions fold to a single and(...) memento.
//
// Layer 2 returns a CLAIM SET of test fn names. The dispatcher passes
// that set to Layer 0's lift_file_with_skip equivalent so the two
// layers PARTITION the test fns, never double-count. When Pattern 3
// finds < 2 atoms liftable, the claim is RELEASED so Layer 0 can
// still mint the individual asserts.
//
// Cross-language conformance: a memento minted by this Go adapter is
// byte-identical (canonical bytes, BLAKE3-512 CID) to the equivalent
// memento minted by the Rust or TS adapter for the same proposition.
// The ir package's MarshalJSON methods own the JCS-friendly key order;
// the canonicalizer/ companion handles the BLAKE3-512 hash widening.
package lifgotests

import (
	"github.com/tsavo/provekit/go/provekit-ir-symbolic/ir"
)

// ADAPTER is the adapter-name tag carried by every warning this layer
// emits. Distinct from a hypothetical "go-tests" Layer 0 tag so a
// reader of the report can tell which layer made each call.
const ADAPTER = "go-tests-layer2"

// LiftWarning is the structured skip/warning record. Mirrors the Rust
// adapter's LiftWarning shape so cross-language reports fold uniformly.
type LiftWarning struct {
	Adapter    string
	SourcePath string
	ItemName   string
	Reason     string
}

// Layer2Output is the result of a Layer 2 pass over a single Go file.
// `Decls` are emitted IR ContractDeclaration values ready to mint;
// `Warnings` are skips with structured reasons; `ClaimedTests` is the
// set of test fn names this pass owns (Layer 0 must skip these).
type Layer2Output struct {
	Decls        []ir.ContractDeclaration
	Warnings     []LiftWarning
	Seen         int
	Lifted       int
	ClaimedTests map[string]struct{}
	// Per-pattern split counts (for CLI summaries / regressions).
	BoundedLoopLifted       int
	BoundedLoopSkipped      int
	HelperInlinedLifted     int
	HelperInlinedSkipped    int
	CharacterizationLifted  int
	CharacterizationSkipped int
}

func newLayer2Output() *Layer2Output {
	return &Layer2Output{
		ClaimedTests: make(map[string]struct{}),
	}
}

func (o *Layer2Output) claim(name string) {
	o.ClaimedTests[name] = struct{}{}
}

func (o *Layer2Output) unclaim(name string) {
	delete(o.ClaimedTests, name)
}

func (o *Layer2Output) IsClaimed(name string) bool {
	_, ok := o.ClaimedTests[name]
	return ok
}

func (o *Layer2Output) warn(sourcePath, itemName, reason string) {
	o.Warnings = append(o.Warnings, LiftWarning{
		Adapter:    ADAPTER,
		SourcePath: sourcePath,
		ItemName:   itemName,
		Reason:     reason,
	})
}
