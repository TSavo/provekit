package verifier

import (
	"fmt"
	"sort"
	"strings"
)

// SMTEmitter translates an IR formula (JSON-shape value tree) to
// SMT-LIB v2.6 for the solver dispatcher. v1 subset: atomic predicates
// over Int (=, ≠, <, ≤, >, ≥), plus recursive descent through
// and/or/not/implies/forall/exists. Sufficient for the parseInt
// precondition demo.
type SMTEmitter struct{}

// NewSMTEmitter returns a fresh emitter.
func NewSMTEmitter() *SMTEmitter { return &SMTEmitter{} }

// EmitProbe builds a complete SMT-LIB script that asks "is (not OBLIGATION) SAT?"
// — the protocol's solver-discharge probe.
//
//   unsat → obligation holds in all models → DISCHARGED
//   sat   → counter-example exists         → UNSATISFIED
func (e *SMTEmitter) EmitProbe(obligation interface{}) (string, error) {
	o, ok := obligation.(map[string]interface{})
	if !ok {
		return "", fmt.Errorf("EmitProbe: obligation is not a JSON object")
	}
	body, err := e.emitFormula(o)
	if err != nil {
		return "", err
	}
	// Collect free variable declarations from the formula.
	freeVars := map[string]string{}
	collectFreeVars(o, freeVars, map[string]bool{})

	var sb strings.Builder
	sb.WriteString("(set-logic ALL)\n")
	keys := make([]string, 0, len(freeVars))
	for k := range freeVars {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	for _, k := range keys {
		fmt.Fprintf(&sb, "(declare-const %s %s)\n", k, freeVars[k])
	}
	fmt.Fprintf(&sb, "(assert (not %s))\n", body)
	sb.WriteString("(check-sat)\n")
	return sb.String(), nil
}

func (e *SMTEmitter) emitFormula(f map[string]interface{}) (string, error) {
	switch f["kind"] {
	case "atomic":
		predicate, _ := f["predicate"].(string)
		args, _ := f["args"].([]interface{})
		smtPred := smtPredicate(predicate)
		argStrs := make([]string, len(args))
		for i, a := range args {
			am, ok := a.(map[string]interface{})
			if !ok {
				return "", fmt.Errorf("atomic arg is not a term")
			}
			s, err := e.emitTerm(am)
			if err != nil {
				return "", err
			}
			argStrs[i] = s
		}
		return fmt.Sprintf("(%s %s)", smtPred, strings.Join(argStrs, " ")), nil
	case "and":
		conjuncts, _ := f["conjuncts"].([]interface{})
		return e.emitConnective("and", conjuncts)
	case "or":
		disjuncts, _ := f["disjuncts"].([]interface{})
		return e.emitConnective("or", disjuncts)
	case "not":
		body, _ := f["body"].(map[string]interface{})
		s, err := e.emitFormula(body)
		if err != nil {
			return "", err
		}
		return fmt.Sprintf("(not %s)", s), nil
	case "implies":
		a, _ := f["antecedent"].(map[string]interface{})
		c, _ := f["consequent"].(map[string]interface{})
		as, err := e.emitFormula(a)
		if err != nil {
			return "", err
		}
		cs, err := e.emitFormula(c)
		if err != nil {
			return "", err
		}
		return fmt.Sprintf("(=> %s %s)", as, cs), nil
	case "forall", "exists":
		// v1: forall/exists in the obligation should have been instantiated
		// already; if a quantifier remains, emit it directly.
		pred, _ := f["predicate"].(map[string]interface{})
		varName, _ := pred["varName"].(string)
		sortV, _ := pred["sort"].(map[string]interface{})
		body, _ := pred["body"].(map[string]interface{})
		bodyS, err := e.emitFormula(body)
		if err != nil {
			return "", err
		}
		return fmt.Sprintf("(%s ((%s %s)) %s)",
			f["kind"], varName, smtSort(sortV), bodyS), nil
	}
	return "", fmt.Errorf("emitFormula: unknown kind %v", f["kind"])
}

func (e *SMTEmitter) emitConnective(op string, terms []interface{}) (string, error) {
	parts := make([]string, len(terms))
	for i, t := range terms {
		tm, _ := t.(map[string]interface{})
		s, err := e.emitFormula(tm)
		if err != nil {
			return "", err
		}
		parts[i] = s
	}
	return fmt.Sprintf("(%s %s)", op, strings.Join(parts, " ")), nil
}

func (e *SMTEmitter) emitTerm(t map[string]interface{}) (string, error) {
	switch t["kind"] {
	case "var":
		name, _ := t["name"].(string)
		return name, nil
	case "const":
		v := t["value"]
		switch val := v.(type) {
		case float64:
			if val == float64(int64(val)) {
				return fmt.Sprintf("%d", int64(val)), nil
			}
			return fmt.Sprintf("%g", val), nil
		case int64:
			return fmt.Sprintf("%d", val), nil
		case bool:
			if val {
				return "true", nil
			}
			return "false", nil
		case string:
			return fmt.Sprintf("%q", val), nil
		default:
			return "", fmt.Errorf("emitTerm const: unsupported value type %T", v)
		}
	case "ctor":
		name, _ := t["name"].(string)
		args, _ := t["args"].([]interface{})
		argStrs := make([]string, len(args))
		for i, a := range args {
			am, ok := a.(map[string]interface{})
			if !ok {
				return "", fmt.Errorf("ctor arg is not a term")
			}
			s, err := e.emitTerm(am)
			if err != nil {
				return "", err
			}
			argStrs[i] = s
		}
		if len(argStrs) == 0 {
			return name, nil
		}
		return fmt.Sprintf("(%s %s)", name, strings.Join(argStrs, " ")), nil
	}
	return "", fmt.Errorf("emitTerm: unknown kind %v", t["kind"])
}

// smtPredicate maps the protocol's predicate name to its SMT-LIB
// equivalent.
func smtPredicate(p string) string {
	switch p {
	case "≠":
		return "distinct"
	case "≤":
		return "<="
	case "≥":
		return ">="
	default:
		return p  // =, <, >, kit-defined predicates passthrough
	}
}

// smtSort renders a Sort JSON-shape value as an SMT-LIB sort.
func smtSort(s map[string]interface{}) string {
	if name, ok := s["name"].(string); ok {
		switch name {
		case "Bool":
			return "Bool"
		case "Real":
			return "Real"
		case "String":
			return "String"
		case "Int":
			return "Int"
		default:
			return name
		}
	}
	return "Int"
}

// collectFreeVars walks a formula, recording each `var` term's name + sort.
// `bound` tracks names introduced by enclosing quantifiers.
func collectFreeVars(f map[string]interface{}, out map[string]string, bound map[string]bool) {
	switch f["kind"] {
	case "atomic":
		args, _ := f["args"].([]interface{})
		for _, a := range args {
			if am, ok := a.(map[string]interface{}); ok {
				collectFreeVarsTerm(am, out, bound)
			}
		}
	case "and":
		for _, c := range arrOf(f, "conjuncts") {
			if cm, ok := c.(map[string]interface{}); ok {
				collectFreeVars(cm, out, bound)
			}
		}
	case "or":
		for _, d := range arrOf(f, "disjuncts") {
			if dm, ok := d.(map[string]interface{}); ok {
				collectFreeVars(dm, out, bound)
			}
		}
	case "not":
		if body, ok := f["body"].(map[string]interface{}); ok {
			collectFreeVars(body, out, bound)
		}
	case "implies":
		if a, ok := f["antecedent"].(map[string]interface{}); ok {
			collectFreeVars(a, out, bound)
		}
		if c, ok := f["consequent"].(map[string]interface{}); ok {
			collectFreeVars(c, out, bound)
		}
	case "forall", "exists":
		pred, _ := f["predicate"].(map[string]interface{})
		varName, _ := pred["varName"].(string)
		newBound := map[string]bool{}
		for k, v := range bound {
			newBound[k] = v
		}
		newBound[varName] = true
		if body, ok := pred["body"].(map[string]interface{}); ok {
			collectFreeVars(body, out, newBound)
		}
	}
}

func collectFreeVarsTerm(t map[string]interface{}, out map[string]string, bound map[string]bool) {
	if t["kind"] == "var" {
		name, _ := t["name"].(string)
		if !bound[name] {
			sortV, _ := t["sort"].(map[string]interface{})
			out[name] = smtSort(sortV)
		}
	}
	if t["kind"] == "ctor" {
		args, _ := t["args"].([]interface{})
		for _, a := range args {
			if am, ok := a.(map[string]interface{}); ok {
				collectFreeVarsTerm(am, out, bound)
			}
		}
	}
}

func arrOf(m map[string]interface{}, key string) []interface{} {
	a, _ := m[key].([]interface{})
	return a
}
