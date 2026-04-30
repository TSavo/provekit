package verifier

import "fmt"

// InstantiateStage substitutes the call-site argument term into the
// outermost forall of the resolved precondition. The result is the
// per-callsite IR-formula obligation the solver discharges.
//
//   forall n: Int. n > 0    instantiated at Const 0
//   →                       0 > 0
type InstantiateStage struct{}

// Run drops the outermost forall and substitutes its bound variable
// with `arg` throughout the body. Returns the substituted formula or
// an error if the input isn't a forall (the precondition must be a
// quantifier in this v1).
func (s *InstantiateStage) Run(formula interface{}, arg interface{}) (interface{}, error) {
	f, ok := formula.(map[string]interface{})
	if !ok {
		return nil, fmt.Errorf("instantiate: formula is not a map")
	}
	if f["kind"] != "forall" {
		return nil, fmt.Errorf("instantiate: outermost is %v, expected forall", f["kind"])
	}
	pred, ok := f["predicate"].(map[string]interface{})
	if !ok {
		return nil, fmt.Errorf("instantiate: forall.predicate missing")
	}
	varName, _ := pred["varName"].(string)
	body, ok := pred["body"].(map[string]interface{})
	if !ok {
		return nil, fmt.Errorf("instantiate: forall body missing")
	}
	return substituteFormula(body, varName, arg), nil
}

// substituteFormula walks a formula tree and replaces var references
// matching `name` with `replacement` (a JSON-shape value of an IrTerm).
func substituteFormula(f map[string]interface{}, name string, replacement interface{}) map[string]interface{} {
	out := cloneMap(f)
	switch f["kind"] {
	case "atomic":
		if args, ok := f["args"].([]interface{}); ok {
			newArgs := make([]interface{}, len(args))
			for i, a := range args {
				if am, ok := a.(map[string]interface{}); ok {
					newArgs[i] = substituteTerm(am, name, replacement)
				} else {
					newArgs[i] = a
				}
			}
			out["args"] = newArgs
		}
	case "and":
		out["conjuncts"] = mapFormulaArr(f["conjuncts"], name, replacement)
	case "or":
		out["disjuncts"] = mapFormulaArr(f["disjuncts"], name, replacement)
	case "not":
		if body, ok := f["body"].(map[string]interface{}); ok {
			out["body"] = substituteFormula(body, name, replacement)
		}
	case "implies":
		if a, ok := f["antecedent"].(map[string]interface{}); ok {
			out["antecedent"] = substituteFormula(a, name, replacement)
		}
		if c, ok := f["consequent"].(map[string]interface{}); ok {
			out["consequent"] = substituteFormula(c, name, replacement)
		}
	case "forall", "exists":
		// Shadowing: don't substitute past a binder that re-introduces `name`.
		if pred, ok := f["predicate"].(map[string]interface{}); ok {
			if pred["varName"] == name {
				return out
			}
			newPred := cloneMap(pred)
			if body, ok := pred["body"].(map[string]interface{}); ok {
				newPred["body"] = substituteFormula(body, name, replacement)
			}
			out["predicate"] = newPred
		}
	}
	return out
}

func substituteTerm(t map[string]interface{}, name string, replacement interface{}) interface{} {
	if t["kind"] == "var" && t["name"] == name {
		return replacement
	}
	if t["kind"] == "ctor" {
		out := cloneMap(t)
		if args, ok := t["args"].([]interface{}); ok {
			newArgs := make([]interface{}, len(args))
			for i, a := range args {
				if am, ok := a.(map[string]interface{}); ok {
					newArgs[i] = substituteTerm(am, name, replacement)
				} else {
					newArgs[i] = a
				}
			}
			out["args"] = newArgs
		}
		return out
	}
	return t
}

func mapFormulaArr(raw interface{}, name string, replacement interface{}) []interface{} {
	arr, _ := raw.([]interface{})
	out := make([]interface{}, len(arr))
	for i, e := range arr {
		if em, ok := e.(map[string]interface{}); ok {
			out[i] = substituteFormula(em, name, replacement)
		} else {
			out[i] = e
		}
	}
	return out
}

func cloneMap(m map[string]interface{}) map[string]interface{} {
	out := make(map[string]interface{}, len(m))
	for k, v := range m {
		out[k] = v
	}
	return out
}
