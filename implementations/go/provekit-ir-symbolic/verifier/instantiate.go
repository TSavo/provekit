package verifier

import "fmt"

// InstantiateStage substitutes the call-site argument term into the
// outermost forall of the resolved precondition. The result is the
// per-callsite IR-formula obligation the solver discharges.
//
//	forall n: Int. n > 0    instantiated at Const 0
//	→                       0 > 0
//
// v1.1.0 IR shape: quantifier is FLAT; {kind, name, sort, body} with
// no Lambda wrapper. The bound variable's identifier is the top-level
// `name` field (was `predicate.varName` pre-v1.1.0).
type InstantiateStage struct{}

// Run drops the outermost forall and substitutes its bound variable
// with `arg` throughout the body. Returns the substituted formula or
// an error if the input isn't a forall.
func (s *InstantiateStage) Run(formula interface{}, arg interface{}) (interface{}, error) {
	f, ok := formula.(map[string]interface{})
	if !ok {
		return nil, fmt.Errorf("instantiate: formula is not a map")
	}
	if f["kind"] != "forall" {
		return nil, fmt.Errorf("instantiate: outermost is %v, expected forall", f["kind"])
	}
	varName, _ := f["name"].(string)
	if varName == "" {
		return nil, fmt.Errorf("instantiate: forall has empty bound-variable name")
	}
	body, ok := f["body"].(map[string]interface{})
	if !ok {
		return nil, fmt.Errorf("instantiate: forall body missing")
	}
	return substituteFormula(body, varName, arg), nil
}

// substituteFormula walks a formula tree (v1.1.0 shape) and replaces
// var references matching `name` with `replacement` (a JSON-shape value
// of an IrTerm).
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
	case "and", "or", "not", "implies":
		if operands, ok := f["operands"].([]interface{}); ok {
			newOps := make([]interface{}, len(operands))
			for i, op := range operands {
				if om, ok := op.(map[string]interface{}); ok {
					newOps[i] = substituteFormula(om, name, replacement)
				} else {
					newOps[i] = op
				}
			}
			out["operands"] = newOps
		}
	case "forall", "exists":
		// Shadowing: don't substitute past a binder that re-introduces `name`.
		if f["name"] == name {
			return out
		}
		if body, ok := f["body"].(map[string]interface{}); ok {
			out["body"] = substituteFormula(body, name, replacement)
		}
	case "choice":
		// Shadowing: don't substitute past a binder that re-introduces `name`.
		if f["varName"] == name {
			return out
		}
		if body, ok := f["body"].(map[string]interface{}); ok {
			out["body"] = substituteFormula(body, name, replacement)
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
	if t["kind"] == "lambda" {
		// Shadowing: don't substitute past a binder that re-introduces `name`.
		if t["paramName"] == name {
			return t
		}
		out := cloneMap(t)
		if body, ok := t["body"].(map[string]interface{}); ok {
			out["body"] = substituteTerm(body, name, replacement)
		}
		return out
	}
	if t["kind"] == "let" {
		out := cloneMap(t)
		shadowed := false
		if bindings, ok := t["bindings"].([]interface{}); ok {
			newBindings := make([]interface{}, len(bindings))
			for i, b := range bindings {
				if bm, ok := b.(map[string]interface{}); ok {
					if !shadowed {
						newBoundTerm := bm["boundTerm"]
						if btm, ok := bm["boundTerm"].(map[string]interface{}); ok {
							newBoundTerm = substituteTerm(btm, name, replacement)
						}
						newBindings[i] = map[string]interface{}{
							"name":      bm["name"],
							"boundTerm": newBoundTerm,
						}
						if bm["name"] == name {
							shadowed = true
						}
					} else {
						newBindings[i] = b
					}
				} else {
					newBindings[i] = b
				}
			}
			out["bindings"] = newBindings
		}
		if !shadowed {
			if body, ok := t["body"].(map[string]interface{}); ok {
				out["body"] = substituteTerm(body, name, replacement)
			}
		}
		return out
	}
	return t
}

func cloneMap(m map[string]interface{}) map[string]interface{} {
	out := make(map[string]interface{}, len(m))
	for k, v := range m {
		out[k] = v
	}
	return out
}
