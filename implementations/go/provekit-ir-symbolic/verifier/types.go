// Package verifier is Go's bridge enforcement workflow — the protocol-
// first verifier. Mirrors the TS impl's 6-stage pipeline:
//   1. load-all-proofs        → unified CID-keyed pool
//   2. enumerate-callsites    → bridge call sites in property mementos
//   3. resolve-bridge-target  → property memento for each callsite
//   4. instantiate-obligation → IR formula at the call site
//   5. solve-obligation       → solver verdict (z3, in parallel via channels)
//   6. report                 → aggregated output
//
// Spec: protocol/specs/2026-04-30-proof-file-format.md +
//       protocol/specs/2026-04-30-chain-validity-and-fail-closed.md
package verifier

// MementoPool is the unified store every downstream stage hash-looks-up
// against. CID → envelope JSON object (parsed). bridgesBySymbol indexes
// bridge envelopes by their evidence.body.sourceSymbol for the
// callsite-enumeration stage.
type MementoPool struct {
	Mementos         map[string]map[string]interface{}  // CID → parsed envelope
	BridgesBySymbol  map[string]map[string]interface{}  // sourceSymbol → bridge envelope
	LoadErrors       []LoadError
}

// LoadError captures per-file failures during load-all-proofs.
type LoadError struct {
	ProofPath string
	Reason    string
}

// CallSite is a (bridge, property memento, arg term) triple — the
// per-call-site obligation downstream stages discharge.
type CallSite struct {
	BridgeIRName        string
	BridgeTargetCID     string
	BridgeSourceLayer   string
	BridgeTargetLayer   string
	PropertyName        string
	PropertyCID         string
	ArgTerm             interface{}  // JSON-shape value of the IrTerm
}

// ResolvedProperty is what resolve-bridge-target returns: the IR
// formula of the contract's `pre` slot. v1.1.0: the resolved memento
// is a contract; scope + kitVersion are gone (the contract memento
// body doesn't carry them).
type ResolvedProperty struct {
	CID       string
	IRFormula interface{} // JSON-shape value; an IrFormula
}

// ObligationVerdict is the outcome of solve-obligation.
type ObligationVerdict string

const (
	VerdictDischarged   ObligationVerdict = "discharged"
	VerdictUnsatisfied  ObligationVerdict = "unsatisfied"
	VerdictUndecidable  ObligationVerdict = "undecidable"
	VerdictDisagreement ObligationVerdict = "disagreement"
)

// ReportRow is the per-callsite outcome.
type ReportRow struct {
	CallSite CallSite
	Status   string  // "discharged" / "unsatisfied" / "unresolved-target" / "lift-error" / etc.
	Reason   string  // optional supporting detail
}

// Report aggregates the per-callsite outcomes.
type Report struct {
	TotalCallsites int
	Discharged     int
	Violations     int
	Rows           []ReportRow
	LoadErrors     []LoadError
}
