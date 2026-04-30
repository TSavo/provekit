package ir

import (
	"fmt"
	"sync"
)

var (
	quantifierMu      sync.Mutex
	quantifierCounter int
)

func resetQuantifierCounter() {
	quantifierMu.Lock()
	quantifierCounter = 0
	quantifierMu.Unlock()
}

func freshVar(sort Sort) varTerm {
	quantifierMu.Lock()
	name := fmt.Sprintf("_x%d", quantifierCounter)
	quantifierCounter++
	quantifierMu.Unlock()
	return varTerm{Name: name, Sort: sort}
}

// ForAll constructs a flat universal quantifier: {kind, name, sort, body}.
// The bound variable is auto-named (_x0, _x1, ...) and passed to the
// body builder.
func ForAll(sort Sort, body func(x IrTerm) IrFormula) IrFormula {
	v := freshVar(sort)
	return quantFormula{
		Kind: "forall",
		Name: v.Name,
		Sort: sort,
		Body: body(v),
	}
}

// Exists constructs a flat existential quantifier: {kind, name, sort, body}.
func Exists(sort Sort, body func(x IrTerm) IrFormula) IrFormula {
	v := freshVar(sort)
	return quantFormula{
		Kind: "exists",
		Name: v.Name,
		Sort: sort,
		Body: body(v),
	}
}

// And builds a connective with operands. The v1.1.0 grammar requires
// 2+ operands for and/or; the legacy 0/1-operand callers collapse to
// the trivial true/atomic equivalents.
func And(formulas ...IrFormula) IrFormula {
	if len(formulas) == 0 {
		return atomicFormula{Name: "true", Args: []IrTerm{}}
	}
	if len(formulas) == 1 {
		return formulas[0]
	}
	return connectiveFormula{Kind: "and", Operands: formulas}
}

// Or builds a connective with operands.
func Or(formulas ...IrFormula) IrFormula {
	if len(formulas) == 0 {
		return atomicFormula{Name: "false", Args: []IrTerm{}}
	}
	if len(formulas) == 1 {
		return formulas[0]
	}
	return connectiveFormula{Kind: "or", Operands: formulas}
}

// Not builds a connective with exactly one operand.
func Not(f IrFormula) IrFormula {
	return connectiveFormula{Kind: "not", Operands: []IrFormula{f}}
}

// Implies builds a connective with exactly two operands. operands[0] is
// the antecedent, operands[1] is the consequent.
func Implies(antecedent, consequent IrFormula) IrFormula {
	return connectiveFormula{Kind: "implies", Operands: []IrFormula{antecedent, consequent}}
}

// Iff desugars to and(implies(a, b), implies(b, a)).
func Iff(a, b IrFormula) IrFormula {
	return And(Implies(a, b), Implies(b, a))
}
