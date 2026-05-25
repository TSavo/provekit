package emitgotesting

import (
	"fmt"
	"go/token"
	"sort"
	"strconv"
	"strings"
	"unicode"

	"github.com/tsavo/provekit/go/provekit-ir-symbolic/canonicalizer"
)

type EmitPlan struct {
	ContractID  string           `json:"contract_id"`
	PackageName string           `json:"package_name"`
	Function    string           `json:"function"`
	Params      []string         `json:"params"`
	ParamTypes  []string         `json:"param_types"`
	ReturnType  string           `json:"return_type"`
	Predicates  []map[string]any `json:"predicates"`
}

type Emission struct {
	Kind                  string   `json:"kind"`
	Source                string   `json:"source"`
	Path                  string   `json:"path"`
	Extension             string   `json:"extension"`
	EmittedArtifactCID    string   `json:"emitted_artifact_cid"`
	EmittedPredicates     []string `json:"emitted_predicates"`
	UnsupportedPredicates []string `json:"unsupported_predicates"`
	IsComplete            bool     `json:"is_complete"`
}

func EmitPlanFromParams(params map[string]any) EmitPlan {
	return EmitPlan{
		ContractID:  firstString(params["contract_id"], params["concept_name"]),
		PackageName: firstString(params["package_name"], params["package"], params["packageName"]),
		Function:    firstString(params["function"], params["function_name"], params["functionName"]),
		Params:      stringList(params["params"]),
		ParamTypes:  stringList(params["param_types"]),
		ReturnType:  firstString(params["return_type"], params["returnType"]),
		Predicates:  mapList(params["predicates"]),
	}
}

func Emit(plan EmitPlan) Emission {
	pkg := sanitizeIdentifier(plan.PackageName, "provekit")
	var functions []string
	var emitted []string
	var unsupported []string
	needsErrors := false

	for i, predicate := range plan.Predicates {
		head := headOf(predicate)
		assertion, ok := renderAssertion(head, predicate)
		if !ok {
			unsupported = append(unsupported, unsupportedName(head))
			continue
		}
		vars := freeVars(predicate)
		decls, declNeedsErrors := declarationsFor(head, vars)
		if declNeedsErrors {
			needsErrors = true
		}
		functions = append(functions, renderTestFunction(head, i, decls, assertion))
		emitted = append(emitted, canonicalHead(head))
	}

	source := renderModule(pkg, functions, needsErrors)
	return Emission{
		Kind:                  "go-testing-test-emission",
		Source:                source,
		Path:                  "provekit_emitted_test.go",
		Extension:             "go",
		EmittedArtifactCID:    canonicalizer.ComputeCID([]byte(source)),
		EmittedPredicates:     emitted,
		UnsupportedPredicates: unsupported,
		IsComplete:            len(unsupported) == 0 && len(emitted) > 0,
	}
}

func renderModule(pkg string, functions []string, needsErrors bool) string {
	var b strings.Builder
	fmt.Fprintf(&b, "package %s\n", pkg)
	if len(functions) > 0 {
		b.WriteByte('\n')
		if needsErrors {
			b.WriteString("import (\n\t\"errors\"\n\t\"testing\"\n)\n")
		} else {
			b.WriteString("import \"testing\"\n")
		}
	}
	for _, fn := range functions {
		b.WriteByte('\n')
		b.WriteString(fn)
		if !strings.HasSuffix(fn, "\n") {
			b.WriteByte('\n')
		}
	}
	return b.String()
}

func renderTestFunction(head string, index int, decls []string, assertion string) string {
	var b strings.Builder
	fmt.Fprintf(&b, "func TestProvekit%s%d(t *testing.T) {\n", camelHead(head), index)
	for _, decl := range decls {
		fmt.Fprintf(&b, "\t%s\n", decl)
	}
	fmt.Fprintf(&b, "\t%s\n", assertion)
	b.WriteString("}\n")
	return b.String()
}

func renderAssertion(head string, predicate map[string]any) (string, bool) {
	args := argsOf(predicate)
	switch canonicalHead(head) {
	case "eq":
		return binaryAssertion(args, "==", "eq")
	case "ne":
		return binaryAssertion(args, "!=", "ne")
	case "lt":
		return binaryAssertion(args, "<", "lt")
	case "gt":
		return binaryAssertion(args, ">", "gt")
	case "le":
		return binaryAssertion(args, "<=", "le")
	case "ge":
		return binaryAssertion(args, ">=", "ge")
	case "option-is-some", "not-null":
		return unaryAssertion(args, "== nil", "option-is-some")
	case "option-is-none":
		return unaryAssertion(args, "!= nil", "option-is-none")
	case "fallible-err":
		if len(args) != 1 {
			return "", false
		}
		x, ok := renderTerm(args[0])
		if !ok {
			return "", false
		}
		return fmt.Sprintf("if %s == nil {\n\t\tt.Fatalf(\"provekit predicate fallible-err failed: expected non-nil error\")\n\t}", x), true
	default:
		return "", false
	}
}

func binaryAssertion(args []map[string]any, op, head string) (string, bool) {
	if len(args) != 2 {
		return "", false
	}
	a, ok := renderTerm(args[0])
	if !ok {
		return "", false
	}
	b, ok := renderTerm(args[1])
	if !ok {
		return "", false
	}
	return fmt.Sprintf("if !(%s %s %s) {\n\t\tt.Fatalf(\"provekit predicate %s failed: %s %s %s\")\n\t}", a, op, b, head, a, op, b), true
}

func unaryAssertion(args []map[string]any, condition, head string) (string, bool) {
	if len(args) != 1 {
		return "", false
	}
	x, ok := renderTerm(args[0])
	if !ok {
		return "", false
	}
	return fmt.Sprintf("if %s %s {\n\t\tt.Fatalf(\"provekit predicate %s failed\")\n\t}", x, condition, head), true
}

func declarationsFor(head string, vars []string) ([]string, bool) {
	decls := make([]string, 0, len(vars))
	needsErrors := false
	for i, name := range vars {
		ident := sanitizeIdentifier(name, fmt.Sprintf("v%d", i))
		switch canonicalHead(head) {
		case "option-is-none":
			decls = append(decls, fmt.Sprintf("var %s any = nil", ident))
		case "option-is-some", "not-null":
			decls = append(decls, fmt.Sprintf("%s := any(1)", ident))
		case "fallible-err":
			needsErrors = true
			decls = append(decls, fmt.Sprintf("%s := errors.New(\"contract error\")", ident))
		default:
			decls = append(decls, fmt.Sprintf("%s := %s", ident, placeholderValue(head, i)))
		}
	}
	return decls, needsErrors
}

func placeholderValue(head string, index int) string {
	switch canonicalHead(head) {
	case "lt":
		if index == 0 {
			return "0"
		}
		return "1"
	case "gt":
		if index == 0 {
			return "1"
		}
		return "0"
	case "ne":
		if index == 0 {
			return "0"
		}
		return "1"
	default:
		return "0"
	}
}

func renderTerm(term map[string]any) (string, bool) {
	kind, _ := term["kind"].(string)
	switch kind {
	case "var":
		name, _ := term["name"].(string)
		ident := sanitizeIdentifier(name, "")
		if ident == "" {
			return "", false
		}
		return ident, true
	case "const":
		return renderConst(term["value"])
	case "op", "ctor":
		return renderApplication(term)
	default:
		return "", false
	}
}

func renderConst(value any) (string, bool) {
	switch v := value.(type) {
	case nil:
		return "nil", true
	case bool:
		if v {
			return "true", true
		}
		return "false", true
	case int:
		return strconv.Itoa(v), true
	case int64:
		return strconv.FormatInt(v, 10), true
	case float64:
		if v == float64(int64(v)) {
			return strconv.FormatInt(int64(v), 10), true
		}
		return strconv.FormatFloat(v, 'g', -1, 64), true
	case string:
		return strconv.Quote(v), true
	default:
		return "", false
	}
}

func renderApplication(term map[string]any) (string, bool) {
	name, _ := term["name"].(string)
	name = strings.TrimPrefix(name, "concept:")
	args := argsOf(term)
	rendered := make([]string, 0, len(args))
	for _, arg := range args {
		r, ok := renderTerm(arg)
		if !ok {
			return "", false
		}
		rendered = append(rendered, r)
	}
	if isArithmetic(name) && len(rendered) == 2 {
		return fmt.Sprintf("(%s %s %s)", rendered[0], name, rendered[1]), true
	}
	ident := sanitizeIdentifier(name, "")
	if ident == "" {
		return "", false
	}
	return fmt.Sprintf("%s(%s)", ident, strings.Join(rendered, ", ")), true
}

func freeVars(term map[string]any) []string {
	var out []string
	seen := map[string]bool{}
	var walk func(map[string]any)
	walk = func(node map[string]any) {
		if node["kind"] == "var" {
			name, _ := node["name"].(string)
			if name != "" && !seen[name] {
				seen[name] = true
				out = append(out, name)
			}
			return
		}
		for _, arg := range argsOf(node) {
			walk(arg)
		}
	}
	walk(term)
	return out
}

func headOf(predicate map[string]any) string {
	name, _ := predicate["name"].(string)
	return strings.TrimPrefix(name, "concept:")
}

func canonicalHead(head string) string {
	switch head {
	case "neq":
		return "ne"
	case "lte":
		return "le"
	case "gte":
		return "ge"
	default:
		return head
	}
}

func unsupportedName(head string) string {
	if head == "" {
		return "<malformed>"
	}
	return head
}

func argsOf(node map[string]any) []map[string]any {
	switch raw := node["args"].(type) {
	case []map[string]any:
		return raw
	case []any:
		out := make([]map[string]any, 0, len(raw))
		for _, item := range raw {
			if m, ok := item.(map[string]any); ok {
				out = append(out, m)
			}
		}
		return out
	default:
		return nil
	}
}

func isArithmetic(name string) bool {
	switch name {
	case "+", "-", "*", "/", "%":
		return true
	default:
		return false
	}
}

func camelHead(head string) string {
	head = canonicalHead(head)
	parts := strings.FieldsFunc(head, func(r rune) bool {
		return r == '-' || r == '_' || r == ':' || r == '.'
	})
	if len(parts) == 0 {
		return "Predicate"
	}
	var b strings.Builder
	for _, part := range parts {
		if part == "" {
			continue
		}
		runes := []rune(part)
		runes[0] = unicode.ToUpper(runes[0])
		b.WriteString(string(runes))
	}
	if b.Len() == 0 {
		return "Predicate"
	}
	return b.String()
}

func sanitizeIdentifier(name, fallback string) string {
	if token.IsIdentifier(name) && !isKeyword(name) {
		return name
	}
	if fallback != "" && token.IsIdentifier(fallback) && !isKeyword(fallback) {
		return fallback
	}
	return ""
}

func isKeyword(name string) bool {
	return token.Lookup(name).IsKeyword()
}

func firstString(values ...any) string {
	for _, value := range values {
		if s, ok := value.(string); ok && strings.TrimSpace(s) != "" {
			return s
		}
	}
	return ""
}

func stringList(value any) []string {
	switch raw := value.(type) {
	case []string:
		return append([]string(nil), raw...)
	case []any:
		out := make([]string, 0, len(raw))
		for _, item := range raw {
			if s, ok := item.(string); ok {
				out = append(out, s)
			}
		}
		return out
	default:
		return nil
	}
}

func mapList(value any) []map[string]any {
	switch raw := value.(type) {
	case []map[string]any:
		return append([]map[string]any(nil), raw...)
	case []any:
		out := make([]map[string]any, 0, len(raw))
		for _, item := range raw {
			if m, ok := item.(map[string]any); ok {
				out = append(out, m)
			}
		}
		return out
	default:
		return nil
	}
}

func supportedPredicates() []string {
	out := []string{
		"concept:eq",
		"concept:ne",
		"concept:lt",
		"concept:gt",
		"concept:le",
		"concept:ge",
		"concept:option-is-some",
		"concept:option-is-none",
		"concept:not-null",
		"concept:fallible-err",
	}
	sort.Strings(out)
	return out
}
