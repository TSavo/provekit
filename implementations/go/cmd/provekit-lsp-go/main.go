// provekit-lsp-go — NDJSON LSP plugin for Go.
//
// Protocol:
//
//	{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
//	{"jsonrpc":"2.0","id":2,"method":"parse","params":{"path":"...","source":"..."}}
//	{"jsonrpc":"2.0","id":3,"method":"shutdown"}
//
// For parse, scans the source for //provekit: annotations and
// go-playground/validator struct tags, lifts to canonical IR,
// and returns JCS declarations JSON.
package main

import (
	"bufio"
	"encoding/json"
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"os"
	"reflect"
	"strings"

	ir "github.com/tsavo/provekit/go/provekit-ir-symbolic/ir"
)

type rpcRequest struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      interface{}     `json:"id"`
	Method  string          `json:"method"`
	Params  json.RawMessage `json:"params"`
}

type parseParams struct {
	Path   string `json:"path"`
	Source string `json:"source"`
}

type rpcResponse struct {
	JSONRPC string      `json:"jsonrpc"`
	ID      interface{} `json:"id"`
	Result  interface{} `json:"result,omitempty"`
	Error   *rpcError   `json:"error,omitempty"`
}

type rpcError struct {
	Code    int    `json:"code"`
	Message string `json:"message"`
}

type initializeResult struct {
	Name         string   `json:"name"`
	Version      string   `json:"version"`
	Capabilities []string `json:"capabilities"`
}

type parseResult struct {
	Declarations json.RawMessage `json:"declarations"`
	Warnings     []interface{}   `json:"warnings"`
}

func main() {
	scanner := bufio.NewScanner(os.Stdin)
	for scanner.Scan() {
		if !handleRequest(string(scanner.Bytes())) {
			return
		}
	}
}

// handleRequest processes a single NDJSON line. Extracted for testability.
// Returns true if the server should continue; false if shutdown was requested.
func handleRequest(line string) bool {
	var req rpcRequest
	if err := json.Unmarshal([]byte(line), &req); err != nil {
		return true
	}

	switch req.Method {
	case "initialize":
		handleInit(req.ID)
	case "parse":
		handleParse(req.ID, req.Params)
	case "shutdown":
		handleShutdown(req.ID)
		return false
	default:
		sendError(req.ID, -32601, fmt.Sprintf("unknown method: %s", req.Method))
	}
	return true
}

func handleInit(id interface{}) {
	send(id, initializeResult{
		Name:         "provekit-lsp-go",
		Version:      "0.1.0",
		Capabilities: []string{"parse"},
	})
}

func handleParse(id interface{}, paramsRaw json.RawMessage) {
	var params parseParams
	if err := json.Unmarshal(paramsRaw, &params); err != nil {
		sendError(id, -32602, "invalid params")
		return
	}

	var decls []ir.Declaration
	warnings := []interface{}{}

	// Walk source for validator structs
	validatorDecls := walkSource(params.Source, params.Path)
	decls = append(decls, validatorDecls...)

	// Scan for //provekit: annotations
	annotationDecls := scanAnnotations(params.Source, params.Path)
	decls = append(decls, annotationDecls...)

	// Marshal declarations
	jcs, err := json.Marshal(decls)
	if err != nil {
		sendError(id, -32603, fmt.Sprintf("marshal: %v", err))
		return
	}
	if len(jcs) == 0 || string(jcs) == "null" {
		jcs = []byte("[]")
	}

	send(id, parseResult{
		Declarations: jcs,
		Warnings:     warnings,
	})
}

func handleShutdown(id interface{}) {
	send(id, nil)
}

// sendResponse is the response writer. Defaults to writing JSON to stdout.
// Overridden in tests to capture output.
var sendResponse = func(resp rpcResponse) {
	b, _ := json.Marshal(resp)
	fmt.Println(string(b))
}

func send(id interface{}, result interface{}) {
	sendResponse(rpcResponse{
		JSONRPC: "2.0",
		ID:      id,
		Result:  result,
	})
}

func sendError(id interface{}, code int, message string) {
	sendResponse(rpcResponse{
		JSONRPC: "2.0",
		ID:      id,
		Error: &rpcError{
			Code:    code,
			Message: message,
		},
	})
}

// walkSource parses Go source and lifts validator struct declarations.
func walkSource(src, path string) []ir.Declaration {
	fset := token.NewFileSet()
	f, err := parser.ParseFile(fset, path, src, 0)
	if err != nil {
		return nil
	}

	var decls []ir.Declaration

	// Walk top-level type declarations for structs with validate tags
	for _, d := range f.Decls {
		genDecl, ok := d.(*ast.GenDecl)
		if !ok || genDecl.Tok != token.TYPE {
			continue
		}
		for _, spec := range genDecl.Specs {
			typeSpec, ok := spec.(*ast.TypeSpec)
			if !ok {
				continue
			}
			structType, ok := typeSpec.Type.(*ast.StructType)
			if !ok {
				continue
			}
			decls = append(decls, liftStructFromAST(typeSpec.Name.Name, structType)...)
		}
	}
	return decls
}

// liftStructFromAST walks struct fields with validate tags and lifts to IR.
// This mirrors the validator.LiftStruct logic but operates on the AST
// rather than requiring a live struct value.
func liftStructFromAST(structName string, st *ast.StructType) []ir.Declaration {
	var decls []ir.Declaration

	for _, field := range st.Fields.List {
		if field.Tag == nil {
			continue
		}
		tag := field.Tag.Value
		// Tag is like `validate:"required,min=1"` — strip outer backticks
		tag = strings.TrimPrefix(tag, "`")
		tag = strings.TrimSuffix(tag, "`")
		tag = strings.TrimSpace(tag)

		// Parse as a struct tag key:"value"
		st := reflect.StructTag(tag)
		validate, ok := st.Lookup("validate")
		if !ok || validate == "" {
			continue
		}

		// Determine Go type kind from AST
		goKind := inferGoKind(field.Type)

		// Multiple field names? (e.g. `a, b int`)
		for _, name := range field.Names {
			// Build IR formula from validate tags
			f := liftValidateTags(goKind, name.Name, validate)
			if f != nil {
				decls = append(decls, ir.ContractDeclaration{
					Name:       fmt.Sprintf("%s.%s", structName, name.Name),
					OutBinding: ir.DefaultOutBinding,
					Pre:        f,
				})
			}
		}
	}
	return decls
}

// inferGoKind maps an AST type expression to a rough Go kind string.
func inferGoKind(expr ast.Expr) string {
	switch t := expr.(type) {
	case *ast.Ident:
		switch t.Name {
		case "string":
			return "string"
		case "int", "int8", "int16", "int32", "int64",
			"uint", "uint8", "uint16", "uint32", "uint64",
			"float32", "float64":
			return "int"
		case "bool":
			return "bool"
		}
	case *ast.StarExpr, *ast.InterfaceType, *ast.MapType, *ast.ArrayType:
		return "ref"
	}
	return "ref"
}

// liftValidateTags maps validate tag fragments to IR formulas.
func liftValidateTags(goKind, varName, tag string) ir.IrFormula {
	var constraints []ir.IrFormula
	v := ir.MakeVar(varName, goSortFromKind(goKind))

	parts := strings.Split(tag, ",")
	for _, part := range parts {
		part = strings.TrimSpace(part)
		if part == "" {
			continue
		}

		f := liftValidateTag(v, goKind, part)
		if f != nil {
			constraints = append(constraints, f)
		}
	}

	switch len(constraints) {
	case 0:
		return nil
	case 1:
		return constraints[0]
	default:
		return ir.And(constraints...)
	}
}

// liftValidateTag maps a single validate tag to an IR formula.
// Duplicated from validator.go for AST-mode operation.
func liftValidateTag(v ir.IrTerm, goKind, tag string) ir.IrFormula {
	if tag == "required" {
		if goKind == "string" {
			return ir.Neq(v, ir.StrConst(""))
		}
		return ir.Neq(v, ir.Num(0))
	}

	for _, op := range []string{"gte=", "lte=", "gt=", "lt=", "eq=", "ne="} {
		if !strings.HasPrefix(tag, op) {
			continue
		}
		numStr := tag[len(op):]
		n := 0
		if _, err := fmt.Sscanf(numStr, "%d", &n); err != nil {
			return nil
		}
		rhs := ir.Num(int64(n))
		switch op[:len(op)-1] {
		case "gte":
			return ir.Gte(v, rhs)
		case "lte":
			return ir.Lte(v, rhs)
		case "gt":
			return ir.Gt(v, rhs)
		case "lt":
			return ir.Lt(v, rhs)
		case "eq":
			return ir.Eq(v, rhs)
		case "ne":
			return ir.Neq(v, rhs)
		}
	}

	if strings.HasPrefix(tag, "min=") {
		var n int
		if _, err := fmt.Sscanf(tag[4:], "%d", &n); err != nil {
			return nil
		}
		ni := ir.Num(int64(n))
		if goKind == "int" {
			return ir.Gte(v, ni)
		}
		return ir.Gte(ir.StringLength(v), ni)
	}
	if strings.HasPrefix(tag, "max=") {
		var n int
		if _, err := fmt.Sscanf(tag[4:], "%d", &n); err != nil {
			return nil
		}
		ni := ir.Num(int64(n))
		if goKind == "int" {
			return ir.Lte(v, ni)
		}
		return ir.Lte(ir.StringLength(v), ni)
	}
	if strings.HasPrefix(tag, "len=") {
		var n int
		if _, err := fmt.Sscanf(tag[4:], "%d", &n); err != nil {
			return nil
		}
		return ir.Eq(ir.StringLength(v), ir.Num(int64(n)))
	}
	if tag == "email" {
		return ir.And() // true placeholder
	}
	if strings.HasPrefix(tag, "oneof=") {
		values := strings.Fields(tag[6:])
		var eqs []ir.IrFormula
		for _, val := range values {
			eqs = append(eqs, ir.Eq(v, ir.StrConst(val)))
		}
		return ir.Or(eqs...)
	}
	return nil
}

// goSortFromKind maps a Go kind string to a ProvekIt Sort.
func goSortFromKind(kind string) ir.Sort {
	switch kind {
	case "string":
		return ir.String
	case "int":
		return ir.Int
	case "bool":
		return ir.Bool
	default:
		return ir.Ref
	}
}

// scanAnnotations scans source lines for //provekit: annotations.
func scanAnnotations(src, path string) []ir.Declaration {
	lines := strings.Split(src, "\n")
	var decls []ir.Declaration

	for i, line := range lines {
		trimmed := strings.TrimSpace(line)

		if strings.HasPrefix(trimmed, "//provekit:contract") {
			fn := findAheadFn(lines, i)
			if fn != "" {
				decls = append(decls, ir.ContractDeclaration{
					Name:       fn,
					OutBinding: ir.DefaultOutBinding,
					Post:       ir.And(), // true placeholder
				})
			}
		}

		if strings.HasPrefix(trimmed, "//provekit:implement") {
			cid := strings.TrimSpace(strings.TrimPrefix(trimmed, "//provekit:implement"))
			fn := findAheadFn(lines, i)
			if fn != "" && cid != "" {
				decls = append(decls, ir.BridgeDeclaration{
					Name:              fn,
					SourceSymbol:      fn,
					SourceLayer:       "go",
					SourceContractCid: "",
					TargetContractCid: cid,
					TargetProofCid:    "",
					TargetLayer:       "rust",
				})
			}
		}
	}
	return decls
}

// findAheadFn scans forward from startLine for a Go function definition.
func findAheadFn(lines []string, startLine int) string {
	maxLine := startLine + 10
	if maxLine >= len(lines) {
		maxLine = len(lines) - 1
	}
	for i := startLine + 1; i <= maxLine && i < len(lines); i++ {
		trimmed := strings.TrimSpace(lines[i])
		// Match: func FuncName(
		if strings.HasPrefix(trimmed, "func ") {
			rest := trimmed[5:]
			end := strings.IndexAny(rest, " ([\n")
			if end < 0 {
				end = len(rest)
			}
			return strings.TrimSpace(rest[:end])
		}
	}
	return ""
}
