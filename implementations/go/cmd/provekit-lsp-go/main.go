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
// and returns JCS declarations JSON alongside call-edge mementos
// per protocol/specs/2026-05-03-bridge-linkage-protocol.md §1 R1.
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

	canonicalizer "github.com/tsavo/provekit/go/provekit-ir-symbolic/canonicalizer"
	ir "github.com/tsavo/provekit/go/provekit-ir-symbolic/ir"
	validator "github.com/tsavo/provekit/go/provekit-lift-go-validator"
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
	CallEdges    json.RawMessage `json:"callEdges"`
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

	// Emit call-edge stream per spec #114 R1.
	// Walk function bodies to find call sites; emit one CallEdgeDeclaration
	// per call site where the calling function has a known contract.
	callEdges := walkCallEdges(params.Source, params.Path, decls)
	edgesJSON, err := json.Marshal(callEdges)
	if err != nil {
		sendError(id, -32603, fmt.Sprintf("marshal call edges: %v", err))
		return
	}
	if len(edgesJSON) == 0 || string(edgesJSON) == "null" {
		edgesJSON = []byte("[]")
	}

	send(id, parseResult{
		Declarations: jcs,
		CallEdges:    edgesJSON,
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

// contractCidForDeclaration returns the BLAKE3-512 CID of a single
// Declaration's canonical JSON bytes. Used to populate sourceContractCid
// and targetContractCid in call-edge mementos.
func contractCidForDeclaration(d ir.Declaration) string {
	body, err := ir.MarshalDeclarations([]ir.Declaration{d})
	if err != nil {
		return ""
	}
	return canonicalizer.ComputeCID(body)
}

// buildContractIndex returns a map from function/contract name to
// (CID, declaration) for each ContractDeclaration in decls.
func buildContractIndex(decls []ir.Declaration) map[string]string {
	idx := make(map[string]string)
	for _, d := range decls {
		if d.Kind() == "contract" {
			cid := contractCidForDeclaration(d)
			if cid != "" {
				idx[d.DeclName()] = cid
			}
		}
	}
	return idx
}

// cgoKitPrefix returns the kit prefix for a cgo call target symbol.
// For the Michael Jordan demo the convention is:
//   - rust-kit: functions marked #[no_mangle] extern "C" in Rust
//   - cpp-kit:  C++ functions
//   - c-kit:    plain C functions
//
// Without build metadata the lifter defaults to "rust-kit" when the
// symbol is not in the local Go contract index, per the spec's primary
// demo case. Future work: a //provekit:cgo-kit hint annotation.
func cgoKitPrefix(funcName string, contractIndex map[string]string) string {
	// If we have a local contract for this name it's same-kit.
	// cgo calls by definition cross kits; always use rust-kit as default
	// per the Michael Jordan demo framing.
	_ = contractIndex
	return "rust-kit"
}

// walkCallEdges parses the Go source and, for every function body whose
// function name has a contract in decls, emits one CallEdgeDeclaration
// per call site within that body per spec #114 §1.
//
// Same-kit calls: both sourceContractCid and targetContractCid populated.
// cgo calls (C.<name>(...)): targetContractCid = null, targetSymbol =
// "<kit>:<name>".
func walkCallEdges(src, path string, decls []ir.Declaration) []ir.CallEdgeDeclaration {
	fset := token.NewFileSet()
	f, err := parser.ParseFile(fset, path, src, 0)
	if err != nil {
		return nil
	}

	contractIndex := buildContractIndex(decls)
	if len(contractIndex) == 0 {
		return nil
	}

	var edges []ir.CallEdgeDeclaration

	for _, d := range f.Decls {
		funcDecl, ok := d.(*ast.FuncDecl)
		if !ok || funcDecl.Name == nil || funcDecl.Body == nil {
			continue
		}

		callerName := funcDecl.Name.Name
		sourceCid, hasCid := contractIndex[callerName]
		if !hasCid {
			// Caller has no contract; skip per R1 (we only emit edges
			// where the source is a lifted contract).
			continue
		}

		// Walk the function body for call expressions.
		ast.Inspect(funcDecl.Body, func(n ast.Node) bool {
			callExpr, ok := n.(*ast.CallExpr)
			if !ok {
				return true
			}

			pos := fset.Position(callExpr.Pos())
			locus := ir.Locus{
				File:   path,
				Line:   pos.Line,
				Column: pos.Column,
			}

			// evidenceTerm: placeholder obligation term. The linker
			// resolves the actual post_B ⊃ pre_A obligation; the lifter
			// emits the structural placeholder per R1.
			evidenceTerm := ir.Atomic("call-site-obligation",
				ir.MakeVar(callerName, ir.String))

			// Detect cgo calls: selector expression "C.<name>" where
			// the package is the synthetic "C" package.
			if sel, ok := callExpr.Fun.(*ast.SelectorExpr); ok {
				if ident, ok := sel.X.(*ast.Ident); ok && ident.Name == "C" {
					// cgo call: cross-kit edge.
					targetName := sel.Sel.Name
					kit := cgoKitPrefix(targetName, contractIndex)
					sym := kit + ":" + targetName
					edges = append(edges, ir.CallEdgeDeclaration{
						SourceContractCid: sourceCid,
						TargetContractCid: nil,
						TargetSymbol:      sym,
						CallSiteLocus:     locus,
						EvidenceTerm:      evidenceTerm,
					})
					return true
				}
			}

			// Same-kit or unresolved Go call.
			calleeName := extractCalleeName(callExpr)
			if calleeName == "" {
				return true
			}
			targetCid, hasTarget := contractIndex[calleeName]
			if hasTarget {
				// Same-kit call: both CIDs known.
				edges = append(edges, ir.CallEdgeDeclaration{
					SourceContractCid: sourceCid,
					TargetContractCid: &targetCid,
					TargetSymbol:      calleeName,
					CallSiteLocus:     locus,
					EvidenceTerm:      evidenceTerm,
				})
			}
			// If the target has no contract we don't emit an edge
			// (the call can't be bridged without a contract on both ends
			// for same-kit calls; cross-kit is covered by the cgo path).
			return true
		})
	}

	return edges
}

// extractCalleeName returns the simple function name from a call
// expression. Returns "" for method calls (x.Foo()) that aren't cgo,
// for function literals, and for other complex expressions.
func extractCalleeName(call *ast.CallExpr) string {
	switch fn := call.Fun.(type) {
	case *ast.Ident:
		return fn.Name
	case *ast.SelectorExpr:
		// Only return the selector for package-qualified calls (not method
		// calls on values). We can't distinguish here without type info, so
		// we return the selector name and let the contract index lookup miss.
		return fn.Sel.Name
	}
	return ""
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

// liftStructFromAST walks struct fields with validate tags and lifts to IR
// by delegating to the shared validator core (task #219).
//
// This is the AST-driven counterpart to validator.LiftStruct: rather than
// requiring a live struct value (reflection), it derives the field's
// ir.Sort from the AST type expression and calls validator.LiftValidateTags,
// the same source-agnostic core used by the batch-CLI lift binary.
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
		structTag := reflect.StructTag(tag)
		validateTag, ok := structTag.Lookup("validate")
		if !ok || validateTag == "" {
			continue
		}

		// Derive Sort from AST type expression
		sort := sortFromASTType(field.Type)

		// Multiple field names? (e.g. `a, b int`)
		for _, name := range field.Names {
			v := ir.MakeVar(name.Name, sort)
			f := validator.LiftValidateTags(v, sort, validateTag)
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

// sortFromASTType reduces an AST type expression to a ProvekIt Sort.
// Idents are forwarded by name to validator.GoSortFromTypeName; pointers,
// interfaces, maps, and arrays fall through to ir.Ref.
func sortFromASTType(expr ast.Expr) ir.Sort {
	if ident, ok := expr.(*ast.Ident); ok {
		return validator.GoSortFromTypeName(ident.Name)
	}
	return ir.Ref
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
