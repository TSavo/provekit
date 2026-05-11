package liftgo

import (
	"bytes"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"go/format"
	"os"
	"path/filepath"
	"strings"
)

func Compile(input CompileInput) (CompileOutput, error) {
	if src, ok := sourceFromSourceUnit(input.IR); ok {
		return CompileOutput{Source: src}, nil
	}
	compiled, err := compileContracts(input.IR)
	if err == nil {
		return compiled, nil
	}
	if len(input.IR) == 1 {
		if compiled, bodyErr := CompileBody(input.IR[0]); bodyErr == nil {
			return compiled, nil
		}
	}
	return CompileOutput{}, err
}

func sourceFromSourceUnit(items []any) (string, bool) {
	for _, item := range items {
		generic, err := toGeneric(item)
		if err != nil {
			continue
		}
		m, ok := generic.(map[string]any)
		if !ok || m["kind"] != "go-source-unit" {
			continue
		}
		term, ok := m["term"].(map[string]any)
		if !ok || term["name"] != "go:source-unit" {
			continue
		}
		args, ok := term["args"].([]any)
		if !ok || len(args) == 0 {
			continue
		}
		bytesSlot, ok := args[0].(map[string]any)
		if !ok || bytesSlot["kind"] != "bytes" || bytesSlot["encoding"] != "hex" {
			continue
		}
		hexValue, ok := bytesSlot["value"].(string)
		if !ok {
			continue
		}
		decoded, err := hex.DecodeString(hexValue)
		if err != nil {
			continue
		}
		return string(decoded), true
	}
	return "", false
}

func compileContracts(items []any) (CompileOutput, error) {
	var contracts []map[string]any
	for _, item := range items {
		generic, err := toGeneric(item)
		if err != nil {
			return CompileOutput{}, err
		}
		m, ok := generic.(map[string]any)
		if ok && m["kind"] == "function-contract" {
			contracts = append(contracts, m)
		}
	}
	if len(contracts) == 0 {
		return CompileOutput{}, errors.New("compile: no go-source-unit or function-contract items found")
	}
	var b strings.Builder
	b.WriteString("package main\n\n")
	for _, contract := range contracts {
		name := shortFuncName(stringField(contract, "fnName"))
		formals := stringSliceField(contract, "formals")
		formalSorts := anySliceField(contract, "formalSorts")
		for len(formalSorts) < len(formals) {
			formalSorts = append(formalSorts, primitiveSortValue("Int"))
		}
		ret := goTypeFromSort(contract["returnSort"])
		b.WriteString("func ")
		b.WriteString(name)
		b.WriteString("(")
		for i, formal := range formals {
			if i > 0 {
				b.WriteString(", ")
			}
			b.WriteString(formal)
			b.WriteByte(' ')
			b.WriteString(goTypeFromSort(formalSorts[i]))
		}
		b.WriteString(")")
		if ret != "" && ret != "struct{}" {
			b.WriteByte(' ')
			b.WriteString(ret)
		}
		b.WriteString(" {\n")
		expr, ok := resultExprFromPost(contract["post"])
		if ok && ret != "" && ret != "struct{}" {
			b.WriteString("\treturn ")
			b.WriteString(expr)
			b.WriteByte('\n')
		}
		b.WriteString("}\n\n")
	}
	formatted, err := format.Source([]byte(b.String()))
	if err != nil {
		return CompileOutput{Source: b.String()}, nil
	}
	return CompileOutput{Source: string(formatted)}, nil
}

func resultExprFromPost(post any) (string, bool) {
	m, ok := post.(map[string]any)
	if !ok || m["kind"] != "atomic" || m["name"] != "=" {
		return "", false
	}
	args, ok := m["args"].([]any)
	if !ok || len(args) != 2 {
		return "", false
	}
	return goExprFromIRTerm(args[1])
}

func CompileBody(term any) (CompileOutput, error) {
	generic, err := toGeneric(term)
	if err != nil {
		return CompileOutput{}, err
	}
	exprTerm, ok := returnExprTerm(generic)
	if !ok {
		return CompileOutput{}, errors.New("compile: body term does not contain a compilable expression")
	}
	expr, ok := goExprFromIRTerm(exprTerm)
	if !ok {
		return CompileOutput{}, errors.New("compile: body expression is not supported")
	}
	vars := varsInTerm(exprTerm)
	ret := goTypeFromBodyTerm(exprTerm)
	var b strings.Builder
	b.WriteString("package main\n\n")
	b.WriteString("func F(")
	for i, name := range vars {
		if i > 0 {
			b.WriteString(", ")
		}
		b.WriteString(name)
		b.WriteString(" int")
	}
	b.WriteString(") ")
	b.WriteString(ret)
	b.WriteString(" {\n")
	b.WriteString("\treturn ")
	b.WriteString(expr)
	b.WriteByte('\n')
	b.WriteString("}\n")
	formatted, err := format.Source([]byte(b.String()))
	if err != nil {
		return CompileOutput{Source: b.String()}, nil
	}
	return CompileOutput{Source: string(formatted)}, nil
}

func returnExprTerm(term any) (any, bool) {
	if _, ok := goExprFromIRTerm(term); ok {
		return term, true
	}
	m, ok := term.(map[string]any)
	if !ok {
		return nil, false
	}
	if m["kind"] != "op" {
		return nil, false
	}
	args := anySliceField(m, "args")
	switch m["name"] {
	case "go:return":
		if len(args) == 1 {
			return args[0], true
		}
	case "go:seq":
		for i := len(args) - 1; i >= 0; i-- {
			if expr, ok := returnExprTerm(args[i]); ok {
				return expr, true
			}
		}
	}
	return nil, false
}

func goExprFromIRTerm(term any) (string, bool) {
	m, ok := term.(map[string]any)
	if !ok {
		return "", false
	}
	switch m["kind"] {
	case "var":
		return stringField(m, "name"), true
	case "identifier":
		return stringField(m, "name"), true
	case "const", "literal":
		return literalSource(m["value"]), true
	case "ctor", "op":
		name := stringField(m, "name")
		args := anySliceField(m, "args")
		compiled := make([]string, len(args))
		for i, arg := range args {
			part, ok := goExprFromIRTerm(arg)
			if !ok {
				return "", false
			}
			compiled[i] = part
		}
		switch name {
		case "go:add":
			return binarySource(compiled, "+")
		case "go:sub":
			return binarySource(compiled, "-")
		case "go:mul":
			return binarySource(compiled, "*")
		case "go:div":
			return binarySource(compiled, "/")
		case "go:mod":
			return binarySource(compiled, "%")
		case "go:eq":
			return binarySource(compiled, "==")
		case "go:ne":
			return binarySource(compiled, "!=")
		case "go:lt":
			return binarySource(compiled, "<")
		case "go:le":
			return binarySource(compiled, "<=")
		case "go:gt":
			return binarySource(compiled, ">")
		case "go:ge":
			return binarySource(compiled, ">=")
		case "go:and":
			return binarySource(compiled, "&&")
		case "go:or":
			return binarySource(compiled, "||")
		case "go:bitand":
			return binarySource(compiled, "&")
		case "go:bitor":
			return binarySource(compiled, "|")
		case "go:bitxor":
			return binarySource(compiled, "^")
		case "go:shl":
			return binarySource(compiled, "<<")
		case "go:shr":
			return binarySource(compiled, ">>")
		case "go:not":
			return unarySource(compiled, "!")
		case "go:bitnot":
			return unarySource(compiled, "^")
		case "go:neg":
			return unarySource(compiled, "-")
		case "go:deref":
			return unarySource(compiled, "*")
		case "go:addr":
			return unarySource(compiled, "&")
		case "go:index":
			if len(args) == 2 {
				return compiled[0] + "[" + compiled[1] + "]", true
			}
		case "go:member":
			if len(args) == 2 {
				base, ok := goExprFromIRTerm(args[0])
				if !ok {
					return "", false
				}
				if field, ok := stringFromNameTerm(args[1]); ok {
					return base + "." + field, true
				}
			}
		}
	}
	return "", false
}

func binarySource(args []string, op string) (string, bool) {
	if len(args) != 2 {
		return "", false
	}
	return "(" + args[0] + " " + op + " " + args[1] + ")", true
}

func unarySource(args []string, op string) (string, bool) {
	if len(args) != 1 {
		return "", false
	}
	return op + args[0], true
}

func stringFromNameTerm(term any) (string, bool) {
	m, ok := term.(map[string]any)
	if !ok {
		return "", false
	}
	switch m["kind"] {
	case "const", "literal":
		if s, ok := m["value"].(string); ok {
			return s, true
		}
	case "identifier":
		return stringField(m, "name"), true
	}
	return "", false
}

func varsInTerm(term any) []string {
	seen := map[string]bool{}
	var out []string
	var walk func(any)
	walk = func(v any) {
		switch x := v.(type) {
		case map[string]any:
			if x["kind"] == "var" {
				name := stringField(x, "name")
				if name != "" && name != "nil" && name != "result" && !seen[name] {
					seen[name] = true
					out = append(out, name)
				}
			}
			for _, child := range anySliceField(x, "args") {
				walk(child)
			}
		case []any:
			for _, child := range x {
				walk(child)
			}
		}
	}
	walk(term)
	return out
}

func goTypeFromBodyTerm(term any) string {
	m, ok := term.(map[string]any)
	if !ok {
		return "int"
	}
	switch m["kind"] {
	case "const", "literal":
		switch v := m["value"].(type) {
		case bool:
			return "bool"
		case string:
			return "string"
		case float64:
			return "float64"
		case json.Number:
			if strings.ContainsAny(v.String(), ".eE") {
				return "float64"
			}
		}
	case "ctor", "op":
		switch stringField(m, "name") {
		case "go:eq", "go:ne", "go:lt", "go:le", "go:gt", "go:ge", "go:and", "go:or", "go:not":
			return "bool"
		}
	}
	return "int"
}

func literalSource(v any) string {
	switch x := v.(type) {
	case string:
		b, _ := json.Marshal(x)
		return string(b)
	case json.Number:
		return x.String()
	case bool:
		if x {
			return "true"
		}
		return "false"
	default:
		return fmt.Sprint(x)
	}
}

func shortFuncName(full string) string {
	if i := strings.LastIndex(full, ")."); i >= 0 {
		return full[i+2:]
	}
	if i := strings.LastIndex(full, "."); i >= 0 {
		return full[i+1:]
	}
	return full
}

func goTypeFromSort(sortValue any) string {
	m, ok := sortValue.(map[string]any)
	if !ok {
		return "int"
	}
	name := stringField(m, "name")
	switch name {
	case "Bool":
		return "bool"
	case "String":
		return "string"
	case "Real":
		return "float64"
	case "Ref":
		return "any"
	case "Unit":
		return ""
	default:
		return "int"
	}
}

func stringField(m map[string]any, key string) string {
	if s, ok := m[key].(string); ok {
		return s
	}
	return ""
}

func stringSliceField(m map[string]any, key string) []string {
	items := anySliceField(m, key)
	out := make([]string, 0, len(items))
	for _, item := range items {
		if s, ok := item.(string); ok {
			out = append(out, s)
		}
	}
	return out
}

func anySliceField(m map[string]any, key string) []any {
	items, ok := m[key].([]any)
	if !ok {
		return nil
	}
	return items
}

func readFile(path string) ([]byte, error) {
	return os.ReadFile(path)
}

func modulePathFor(root string) string {
	data, err := os.ReadFile(filepath.Join(root, "go.mod"))
	if err != nil {
		return "command-line-arguments"
	}
	for _, line := range strings.Split(string(data), "\n") {
		fields := strings.Fields(line)
		if len(fields) == 2 && fields[0] == "module" {
			return fields[1]
		}
	}
	return "command-line-arguments"
}

func packagePathFor(modulePath, root, path string) string {
	if modulePath == "" || modulePath == "command-line-arguments" {
		return modulePath
	}
	dir := filepath.Dir(path)
	rel, err := filepath.Rel(root, dir)
	if err != nil || rel == "." {
		return modulePath
	}
	return modulePath + "/" + filepath.ToSlash(rel)
}

func sourceUnitIRFromJSON(raw json.RawMessage) ([]any, error) {
	dec := json.NewDecoder(bytes.NewReader(raw))
	dec.UseNumber()
	var out []any
	if err := dec.Decode(&out); err != nil {
		return nil, err
	}
	return out, nil
}
