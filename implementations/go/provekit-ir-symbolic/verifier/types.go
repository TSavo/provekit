// Package verifier is Go's bridge enforcement workflow: the protocol-
// first verifier. Mirrors the TS impl's 6-stage pipeline:
//  1. load-all-proofs        → unified CID-keyed pool
//  2. enumerate-callsites    → bridge call sites in property mementos
//  3. resolve-bridge-target  → property memento for each callsite
//  4. instantiate-obligation → IR formula at the call site
//  5. solve-obligation       → solver verdict (z3, in parallel via channels)
//  6. report                 → aggregated output
//
// Spec: protocol/specs/2026-04-30-proof-file-format.md +
//
//	protocol/specs/2026-04-30-chain-validity-and-fail-closed.md
package verifier

import (
	"fmt"

	"github.com/tsavo/provekit/go/provekit-ir-symbolic/canonicalizer"
)

// MementoPool is the unified store every downstream stage hash-looks-up
// against. The memento IS the verification. The .proof protocol IS the cache.
// The hash IS the boundary.
//
// CID → envelope JSON object (parsed). bridgesBySymbol indexes
// bridge envelopes by their evidence.body.sourceSymbol for the
// callsite-enumeration stage.
// formulaToMemento indexes formula CIDs → memento CIDs for O(1) verification.
//
// BundleMembers tracks bundle CID → set of member CIDs for the forward
// pin (BridgeDeclaration.ConsequentBundlePinned, NORMATIVE: see
// protocol/specs/2026-04-30-ir-formal-grammar.md § "Bridge target
// pinning: the shim-poisoning vector"). Multi-valued because the same
// member CID can legitimately appear in more than one bundle (one
// honest, one poisoned); last-writer-wins would silently swap them.
// Populated by load_all_proofs from the .proof file's content hash;
// consumed by resolve_target.
type MementoPool struct {
	Mementos         map[string]map[string]interface{} // CID → parsed envelope
	FormulaToMemento map[string]string                 // formula CID → memento CID
	BridgesBySymbol  map[string]map[string]interface{} // sourceSymbol → bridge envelope
	BundleMembers    map[string]map[string]struct{}    // bundle CID → set of member CIDs
	LoadErrors       []LoadError
}

// NewMementoPool creates an empty pool.
func NewMementoPool() *MementoPool {
	return &MementoPool{
		Mementos:         map[string]map[string]interface{}{},
		FormulaToMemento: map[string]string{},
		BridgesBySymbol:  map[string]map[string]interface{}{},
		BundleMembers:    map[string]map[string]struct{}{},
	}
}

// VerifyByHash looks up a formula by its content hash.
// The memento IS the verification; if found, the formula is verified.
// No solver is invoked.
func (p *MementoPool) VerifyByHash(formulaCID string) (map[string]interface{}, bool) {
	mementoCID, ok := p.FormulaToMemento[formulaCID]
	if !ok {
		return nil, false
	}
	memento, ok := p.Mementos[mementoCID]
	return memento, ok
}

// Verify computes the CID for a formula JSON value, then looks it up.
// The canonicalization + hash IS the boundary between systems.
func (p *MementoPool) Verify(formula interface{}) (map[string]interface{}, bool) {
	cid := ComputeFormulaCID(formula)
	return p.VerifyByHash(cid)
}

// VerifyImplication checks if antecedent → consequent is already proven
// in the pool. Looks for an implication memento whose evidence body
// contains both antecedentHash and consequentHash.
func (p *MementoPool) VerifyImplication(antecedentCID, consequentCID string) (map[string]interface{}, bool) {
	for _, envelope := range p.Mementos {
		evidence, ok := envelope["evidence"].(map[string]interface{})
		if !ok {
			continue
		}
		if evidence["kind"] != "implication" {
			continue
		}
		body, ok := evidence["body"].(map[string]interface{})
		if !ok {
			continue
		}
		ant, ok1 := body["antecedentHash"].(string)
		con, ok2 := body["consequentHash"].(string)
		if ok1 && ok2 && ant == antecedentCID && con == consequentCID {
			return envelope, true
		}
	}
	return nil, false
}

// ImplicationResult is the outcome of a CanImply check.
type ImplicationResult int

const (
	ImplicationUnknown ImplicationResult = iota
	ImplicationProvenDirect
	ImplicationProvenTransitive
	ImplicationProvenReflexive
)

// CanImply checks if antecedent → consequent holds via:
//  1. Direct implication memento
//  2. Transitive chaining
//  3. Reflexivity (P → P)
func (p *MementoPool) CanImply(antecedentCID, consequentCID string) (ImplicationResult, []string) {
	if antecedentCID == consequentCID {
		return ImplicationProvenReflexive, []string{antecedentCID}
	}

	// 1. Direct
	if _, ok := p.VerifyImplication(antecedentCID, consequentCID); ok {
		return ImplicationProvenDirect, []string{antecedentCID, consequentCID}
	}

	// 2. Transitive: build graph and BFS
	graph := map[string][]string{}
	for _, envelope := range p.Mementos {
		evidence, ok := envelope["evidence"].(map[string]interface{})
		if !ok || evidence["kind"] != "implication" {
			continue
		}
		body, ok := evidence["body"].(map[string]interface{})
		if !ok {
			continue
		}
		ant, ok1 := body["antecedentHash"].(string)
		con, ok2 := body["consequentHash"].(string)
		if ok1 && ok2 {
			graph[ant] = append(graph[ant], con)
		}
	}

	visited := map[string]bool{}
	queue := [][]string{{antecedentCID}}

	for len(queue) > 0 {
		path := queue[0]
		queue = queue[1:]
		current := path[len(path)-1]

		if visited[current] {
			continue
		}
		visited[current] = true

		for _, neighbor := range graph[current] {
			newPath := append([]string{}, path...)
			newPath = append(newPath, neighbor)
			if neighbor == consequentCID {
				return ImplicationProvenTransitive, newPath
			}
			queue = append(queue, newPath)
		}
	}

	return ImplicationUnknown, nil
}

// Insert adds a memento to the pool and indexes it by formula hash.
// The .proof protocol IS the cache: storing a memento IS caching
// the verification result.
func (p *MementoPool) Insert(mementoCID string, envelope map[string]interface{}) {
	// Index by formula hashes in evidence body
	if evidence, ok := envelope["evidence"].(map[string]interface{}); ok {
		if body, ok := evidence["body"].(map[string]interface{}); ok {
			// Contract evidence
			for _, field := range []string{"preHash", "postHash", "invHash"} {
				if hash, ok := body[field].(string); ok {
					p.FormulaToMemento[hash] = mementoCID
				}
			}
			// Implication evidence
			for _, field := range []string{"antecedentHash", "consequentHash"} {
				if hash, ok := body[field].(string); ok {
					p.FormulaToMemento[hash] = mementoCID
				}
			}
		}
	}
	p.Mementos[mementoCID] = envelope
}

// FindVerifiedSubformulas walks a formula DAG and returns all sub-formula
// CIDs that have mementos in the pool. If P is verified and we need to
// prove P ∧ Q, this returns P's CID so the solver can focus on Q.
func (p *MementoPool) FindVerifiedSubformulas(formula interface{}) []struct {
	CID     string
	Memento map[string]interface{}
} {
	var verified []struct {
		CID     string
		Memento map[string]interface{}
	}
	stack := []interface{}{formula}
	visited := map[string]bool{}

	for len(stack) > 0 {
		node := stack[len(stack)-1]
		stack = stack[:len(stack)-1]

		cid := ComputeFormulaCID(node)
		if visited[cid] {
			continue
		}
		visited[cid] = true

		if memento, ok := p.VerifyByHash(cid); ok {
			verified = append(verified, struct {
				CID     string
				Memento map[string]interface{}
			}{CID: cid, Memento: memento})
		}

		// Push children
		if obj, ok := node.(map[string]interface{}); ok {
			kind, _ := obj["kind"].(string)
			switch kind {
			case "and", "or", "not", "implies":
				if ops, ok := obj["operands"].([]interface{}); ok {
					for _, op := range ops {
						stack = append(stack, op)
					}
				}
			case "forall", "exists", "choice":
				if body, ok := obj["body"]; ok {
					stack = append(stack, body)
				}
			}
		}
	}

	return verified
}

// ComputeFormulaCID canonicalizes a formula JSON value and hashes it.
// The hash IS the boundary: this function is the gate between the
// formula domain and the hash domain.
func ComputeFormulaCID(formula interface{}) string {
	enc := canonicalizer.NewEncoder()
	bytes, err := enc.Encode(formula)
	if err != nil {
		// Fallback: use fmt.Sprintf for non-encodable values
		return canonicalizer.ComputeCID([]byte(fmt.Sprintf("%v", formula)))
	}
	return canonicalizer.ComputeCID(bytes)
}

// LoadError captures per-file failures during load-all-proofs.
type LoadError struct {
	ProofPath string
	Reason    string
}

// CallSite is a (bridge, property memento, arg term) triple: the
// per-call-site obligation downstream stages discharge.
//
// BridgeSourceContractCID and BridgeTargetProofCID mirror the
// BridgeDeclaration fields in the IR formal grammar (PR #10).
// BridgeTargetProofCID is the bridge's pinned consequent .proof bundle
// CID; resolve_target enforces ConsequentBundlePinned against it
// (mirrors Rust PR #13, protocol/specs/2026-04-30-ir-formal-grammar.md
// § "Bridge target pinning: the shim-poisoning vector"). Empty string
// is the back-compat shape: legacy bridges that pre-date the field
// load and resolve, but ConsequentBundlePinned cannot be enforced and
// resolve_target emits a soft warning. BridgeSourceContractCID is
// stored only; reverse-pin enforcement (LiftedFromContract) is owned
// by future verifier work.
type CallSite struct {
	BridgeIRName            string
	BridgeTargetCID         string
	BridgeSourceLayer       string
	BridgeSourceContractCID string
	BridgeTargetProofCID    string
	BridgeTargetLayer       string
	PropertyName            string
	PropertyCID             string
	ArgTerm                 interface{} // JSON-shape value of the IrTerm
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
	Status   string // "discharged" / "unsatisfied" / "unresolved-target" / "lift-error" / etc.
	Reason   string // optional supporting detail
}

// Report aggregates the per-callsite outcomes.
type Report struct {
	TotalCallsites int
	Discharged     int
	Violations     int
	Rows           []ReportRow
	LoadErrors     []LoadError
}
