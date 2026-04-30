package verifier

import (
	"fmt"
	"sort"
	"strings"
)

// SMTEmitter translates an IR formula (JSON-shape value tree) to
// SMT-LIB v2.6 for the solver dispatcher.
//
// v1.1.0 IR shape consumed:
//
//	atomic     {kind:"atomic", name, args}
//	connective {kind:"and"|"or"|"not"|"implies", operands}
//	quantifier {kind:"forall"|"exists", name, sort, body}
//	var        {kind:"var", name}                 -- no sort
//	const      {kind:"const", value, sort}
//	ctor       {kind:"ctor", name, args}          -- no sort
type SMTEmitter struct{}

// NewSMTEmitter returns a fresh emitter.
func NewSMTEmitter() *SMTEmitter { return &SMTEmitter{} }

// EmitProbe builds a complete SMT-LIB script that asks "is (not OBLIGATION) SAT?"
// — the protocol's solver-discharge probe.
//
//	unsat → obligation holds in all models → DISCHARGED
//	sat   → counter-example exists         → UNSATISFIED
func (e *SMTEmitter) EmitProbe(obligation interface{}) (string, error) {
	o, ok := obligation.(map[string]interface{})
	if !ok {
		return "", fmt.Errorf("EmitProbe: obligation is not a JSON object")
	}
	body, err := e.emitFormula(o)
	if err != nil {
		return "", err
	}
	// Collect free variable declarations from the formula. Var terms no
	// longer carry a sort field under v1.1.0, so default to Int (the only
	// quantifier sort the parseInt demo uses). Future commits may infer
	// from enclosing-quantifier sort context.
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
		// v1.1.0: atomic uses `name` (was `predicate`).
		name, _ := f["name"].(string)
		smtPred := smtPredicate(name)
		args, _ := f["args"].([]interface{})
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
	case "and", "or":
		operands, _ := f["operands"].([]interface{})
		return e.emitConnective(f["kind"].(string), operands)
	case "not":
		operands, _ := f["operands"].([]interface{})
		if len(operands) != 1 {
			return "", fmt.Errorf("not: expected 1 operand, got %d", len(operands))
		}
		om, _ := operands[0].(map[string]interface{})
		s, err := e.emitFormula(om)
		if err != nil {
			return "", err
		}
		return fmt.Sprintf("(not %s)", s), nil
	case "implies":
		operands, _ := f["operands"].([]interface{})
		if len(operands) != 2 {
			return "", fmt.Errorf("implies: expected 2 operands, got %d", len(operands))
		}
		am, _ := operands[0].(map[string]interface{})
		cm, _ := operands[1].(map[string]interface{})
		as, err := e.emitFormula(am)
		if err != nil {
			return "", err
		}
		cs, err := e.emitFormula(cm)
		if err != nil {
			return "", err
		}
		return fmt.Sprintf("(=> %s %s)", as, cs), nil
	case "forall", "exists":
		// Flat shape: {kind, name, sort, body} — no Lambda wrapper.
		varName, _ := f["name"].(string)
		sortV, _ := f["sort"].(map[string]interface{})
		body, _ := f["body"].(map[string]interface{})
		bodyS, err := e.emitFormula(body)
		if err != nil {
			return "", err
		}
		return fmt.Sprintf("(%s ((%s %s)) %s)",
			f["kind"], varName, smtSort(sortV), bodyS), nil
	}
	return "", fmt.Errorf("emitFormula: unknown kind %v", f["kind"])
}

func (e *SMTEmitter) emitConnective(op string, operands []interface{}) (string, error) {
	parts := make([]string, len(operands))
	for i, op := range operands {
		om, _ := op.(map[string]interface{})
		s, err := e.emitFormula(om)
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

// smtPredicate maps the protocol's atomic name to its SMT-LIB form.
func smtPredicate(p string) string {
	switch p {
	case "≠":
		return "distinct"
	case "≤":
		return "<="
	case "≥":
		return ">="
	default:
		return p // =, <, >, kit-defined names passthrough
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

// collectFreeVars walks a formula, recording each `var` term's name.
// Var terms no longer carry sort under v1.1.0, so we default to Int
// (the only quantifier sort the parseInt PoC needs).
func collectFreeVars(f map[string]interface{}, out map[string]string, bound map[string]bool) {
	switch f["kind"] {
	case "atomic":
		args, _ := f["args"].([]interface{})
		for _, a := range args {
			if am, ok := a.(map[string]interface{}); ok {
				collectFreeVarsTerm(am, out, bound)
			}
		}
	case "and", "or", "not", "implies":
		operands, _ := f["operands"].([]interface{})
		for _, op := range operands {
			if om, ok := op.(map[string]interface{}); ok {
				collectFreeVars(om, out, bound)
			}
		}
	case "forall", "exists":
		newBound := map[string]bool{}
		for k, v := range bound {
			newBound[k] = v
		}
		varName, _ := f["name"].(string)
		newBound[varName] = true
		if body, ok := f["body"].(map[string]interface{}); ok {
			collectFreeVars(body, out, newBound)
		}
	}
}

func collectFreeVarsTerm(t map[string]interface{}, out map[string]string, bound map[string]bool) {
	if t["kind"] == "var" {
		name, _ := t["name"].(string)
		if !bound[name] {
			out[name] = "Int"
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
