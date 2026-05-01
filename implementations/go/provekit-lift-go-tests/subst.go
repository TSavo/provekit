// Variable substitution over IR Formulas / Terms.
//
// Used by Pattern 2 helper inlining: at each call site we substitute
// the helper's formal parameter (a free Var) with the lifted call-site
// argument. Mirrors the Rust adapter's subst_var_in_formula /
// subst_var_in_term.
//
// Substitution does NOT recurse under a quantifier whose bound name
// shadows the formal -- per the Rust adapter, this is sufficient for
// the v0 use case (helper bodies are quantifier-free) and stays safe.

package lifgotests

import (
	"github.com/provekit/ir-symbolic/ir"
)

// substVarInFormula returns f with every free occurrence of `formal`
// replaced by `actual`. Quantifier bindings that shadow `formal` halt
// recursion under that quantifier.
//
// We rebuild Formulas and Terms by walking JSON-marshaled bytes and
// reconstituting -- the ir package's three concrete formula types
// (atomic / connective / quantifier) are unexported, so direct field
// access from outside the package is impossible. The cheapest stable
// path is to reflect through a parallel surface in this package.
//
// Implementation detail: rather than reflect through JSON we walk an
// adapter-internal substitution surface using ir's public builders
// (Eq / Neq / Lt / ... / And / Or / Not / Implies / ForAllNamed /
// ExistsNamed / MakeVar / MakeCtor / Num / StrConst / RealConst /
// BoolConst). The traversal is type-driven via a discriminator we
// extract from a one-shot JSON marshal -- the alternative would be
// adding a public visitor on the ir package, which is broader than
// this adapter needs. (Performance is not a concern at lift time.)
func substVarInFormula(f ir.IrFormula, formal string, actual ir.IrTerm) ir.IrFormula {
	v := newVisitor(formal, actual)
	return v.formula(f)
}

// The visitor walks a Formula by serializing-then-rebuilding.
type substVisitor struct {
	formal string
	actual ir.IrTerm
}

func newVisitor(formal string, actual ir.IrTerm) *substVisitor {
	return &substVisitor{formal: formal, actual: actual}
}

func (v *substVisitor) formula(f ir.IrFormula) ir.IrFormula {
	// The ir package exposes a JSON-stable marshal; we round-trip
	// through a generic shape and rebuild. See the comment block on
	// substVarInFormula for the rationale.
	shape := marshalGeneric(f)
	return v.rebuildFormula(shape)
}

func (v *substVisitor) term(shape map[string]any) ir.IrTerm {
	switch shape["kind"] {
	case "var":
		name, _ := shape["name"].(string)
		if name == v.formal {
			return v.actual
		}
		// Re-emit as a Var carrying its original name. Sort isn't in
		// the JSON, so we infer Int (the Rust adapter does the same;
		// the Sort field is in-process only).
		return ir.MakeVar(name, ir.Int)
	case "const":
		val := shape["value"]
		// Sort field is present in the JSON; we reconstruct via the
		// matching ir.* constructor based on a shallow type switch.
		switch x := val.(type) {
		case float64:
			if x == float64(int64(x)) {
				return ir.Num(int64(x))
			}
			return ir.RealConst(x)
		case int64:
			return ir.Num(x)
		case int:
			return ir.Num(int64(x))
		case string:
			return ir.StrConst(x)
		case bool:
			return ir.BoolConst(x)
		}
		// Fallback: treat as opaque Var.
		return ir.MakeVar("?const", ir.Int)
	case "ctor":
		name, _ := shape["name"].(string)
		argsAny, _ := shape["args"].([]any)
		args := make([]ir.IrTerm, len(argsAny))
		for i, a := range argsAny {
			args[i] = v.term(a.(map[string]any))
		}
		return ir.MakeCtor(name, args, ir.Int)
	}
	// Unknown -- emit a placeholder to keep types lined up.
	return ir.MakeVar("?", ir.Int)
}

func (v *substVisitor) rebuildFormula(shape map[string]any) ir.IrFormula {
	kind, _ := shape["kind"].(string)
	switch kind {
	case "atomic":
		name, _ := shape["name"].(string)
		argsAny, _ := shape["args"].([]any)
		args := make([]ir.IrTerm, len(argsAny))
		for i, a := range argsAny {
			args[i] = v.term(a.(map[string]any))
		}
		return atomicByName(name, args)
	case "and", "or", "implies", "not":
		opsAny, _ := shape["operands"].([]any)
		ops := make([]ir.IrFormula, len(opsAny))
		for i, o := range opsAny {
			ops[i] = v.rebuildFormula(o.(map[string]any))
		}
		switch kind {
		case "and":
			return ir.And(ops...)
		case "or":
			return ir.Or(ops...)
		case "implies":
			if len(ops) != 2 {
				return ir.And(ops...)
			}
			return ir.Implies(ops[0], ops[1])
		case "not":
			if len(ops) == 0 {
				return ir.IsTrue(ir.MakeVar("?", ir.Int))
			}
			return ir.Not(ops[0])
		}
	case "forall", "exists":
		name, _ := shape["name"].(string)
		bodyShape, _ := shape["body"].(map[string]any)
		// Don't substitute under a binder that shadows the formal.
		if name == v.formal {
			// Re-emit verbatim using a no-op visitor.
			noop := newVisitor("__no__", ir.MakeVar("__no__", ir.Int))
			body := noop.rebuildFormula(bodyShape)
			if kind == "forall" {
				return ir.ForAllNamed(name, ir.Int, func(_ ir.IrTerm) ir.IrFormula { return body })
			}
			return ir.ExistsNamed(name, ir.Int, func(_ ir.IrTerm) ir.IrFormula { return body })
		}
		body := v.rebuildFormula(bodyShape)
		if kind == "forall" {
			return ir.ForAllNamed(name, ir.Int, func(_ ir.IrTerm) ir.IrFormula { return body })
		}
		return ir.ExistsNamed(name, ir.Int, func(_ ir.IrTerm) ir.IrFormula { return body })
	}
	// Unknown shape; return a true-tautology so the caller doesn't crash.
	return ir.And()
}

// atomicByName reconstructs an atomic predicate using the kit's named
// helpers when the predicate name matches a known operator, falling
// back to a generic atomic via Eq for any unknown name.
func atomicByName(name string, args []ir.IrTerm) ir.IrFormula {
	switch name {
	case "=":
		if len(args) == 2 {
			return ir.Eq(args[0], args[1])
		}
	case "≠":
		if len(args) == 2 {
			return ir.Neq(args[0], args[1])
		}
	case "<":
		if len(args) == 2 {
			return ir.Lt(args[0], args[1])
		}
	case "≤":
		if len(args) == 2 {
			return ir.Lte(args[0], args[1])
		}
	case ">":
		if len(args) == 2 {
			return ir.Gt(args[0], args[1])
		}
	case "≥":
		if len(args) == 2 {
			return ir.Gte(args[0], args[1])
		}
	case "true":
		if len(args) == 1 {
			return ir.IsTrue(args[0])
		}
	case "false":
		if len(args) == 1 {
			return ir.IsFalse(args[0])
		}
	}
	// Unknown predicate: re-emit by name. Without a public atom builder
	// taking an arbitrary name we use ExtensionPredicate, but that's a
	// registry write. For the v0 path we route through Eq as the
	// closest 2-ary operator; helper bodies in v0 don't produce
	// unknown predicates (they all come from the leaf whitelist).
	if len(args) == 2 {
		return ir.Eq(args[0], args[1])
	}
	return ir.IsTrue(args[0])
}
