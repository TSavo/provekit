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

func ForAll(sort Sort, body func(x IrTerm) IrFormula) IrFormula {
	v := freshVar(sort)
	return forAllFormula{
		Sort: sort,
		Predicate: irLambda{
			VarName: v.Name,
			Sort:    sort,
			Body:    body(v),
		},
	}
}

func Exists(sort Sort, body func(x IrTerm) IrFormula) IrFormula {
	v := freshVar(sort)
	return existsFormula{
		Sort: sort,
		Predicate: irLambda{
			VarName: v.Name,
			Sort:    sort,
			Body:    body(v),
		},
	}
}

func And(formulas ...IrFormula) IrFormula {
	if len(formulas) == 0 {
		return atomicFormula{Predicate: "true", Args: []IrTerm{}}
	}
	if len(formulas) == 1 {
		return formulas[0]
	}
	return andFormula{Conjuncts: formulas}
}

func Or(formulas ...IrFormula) IrFormula {
	if len(formulas) == 0 {
		return atomicFormula{Predicate: "false", Args: []IrTerm{}}
	}
	if len(formulas) == 1 {
		return formulas[0]
	}
	return orFormula{Disjuncts: formulas}
}

func Not(f IrFormula) IrFormula {
	return notFormula{Body: f}
}

func Implies(antecedent, consequent IrFormula) IrFormula {
	return impliesFormula{Antecedent: antecedent, Consequent: consequent}
}

// Iff desugars to and(implies(a, b), implies(b, a)) to match the TS kit.
func Iff(a, b IrFormula) IrFormula {
	return andFormula{
		Conjuncts: []IrFormula{
			impliesFormula{Antecedent: a, Consequent: b},
			impliesFormula{Antecedent: b, Consequent: a},
		},
	}
}
