// provekit-lsp-go: NDJSON LSP plugin for Go.
//
// Protocol:
//
//	{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
//	{"jsonrpc":"2.0","id":2,"method":"parse","params":{"path":"...","source":"..."}}
//	{"jsonrpc":"2.0","id":3,"method":"analyzeDocument","params":{"file":"...","text":"..."}}
//	{"jsonrpc":"2.0","id":4,"method":"shutdown"}
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
	"strconv"
	"strings"

	canonicalizer "github.com/tsavo/provekit/go/provekit-ir-symbolic/canonicalizer"
	ir "github.com/tsavo/provekit/go/provekit-ir-symbolic/ir"
	validator "github.com/tsavo/provekit/go/provekit-lift-go-validator"
)

const (
	goKitID                     = "go"
	sharedLSPProtocolVersion    = "provekit-lsp-shared/1"
	sharedLSPProtocolCatalogCID = "blake3-512:0e3905c2a7a098cd538b9669428a7dffd2b84ba8ccf8fde3724fe2ab61fd3fbc1e1a616a6b20b6817464cdc50c466b5497d4ac2e2dc34c3c15f05535b463643c"
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

type analyzeDocumentParams struct {
	KitID  string `json:"kit_id"`
	URI    string `json:"uri"`
	File   string `json:"file"`
	Path   string `json:"path"`
	Text   string `json:"text"`
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
	Name               string          `json:"name"`
	Version            string          `json:"version"`
	ProtocolVersion    string          `json:"protocol_version"`
	KitID              string          `json:"kit_id"`
	ProtocolCatalogCID string          `json:"protocol_catalog_cid"`
	Capabilities       lspCapabilities `json:"capabilities"`
}

type lspCapabilities struct {
	SourceSurfaces  []string `json:"source_surfaces"`
	EntryKinds      []string `json:"entry_kinds"`
	DiagnosticCodes []string `json:"diagnostic_codes"`
	StatusKinds     []string `json:"status_kinds"`
}

type parseResult struct {
	Declarations json.RawMessage   `json:"declarations"`
	CallEdges    json.RawMessage   `json:"callEdges"`
	Diagnostics  []LSPDiagnostic   `json:"diagnostics"`
	ContractCids map[string]string `json:"contractCids,omitempty"`
	Warnings     []interface{}     `json:"warnings"`
}

type sourceRange struct {
	StartLine int `json:"start_line"`
	StartCol  int `json:"start_col"`
	EndLine   int `json:"end_line"`
	EndCol    int `json:"end_col"`
}

type lspDocumentEntry struct {
	Kind  string      `json:"kind"`
	Entry interface{} `json:"entry"`
	Range sourceRange `json:"range"`
}

type sharedDiagnostic struct {
	Code               string      `json:"code"`
	Message            string      `json:"message"`
	Severity           string      `json:"severity"`
	Range              sourceRange `json:"range"`
	Producer           string      `json:"producer"`
	KitID              string      `json:"kit_id"`
	ProtocolCatalogCID string      `json:"protocol_catalog_cid"`
	Data               interface{} `json:"data,omitempty"`
}

type lspDocumentAnalysisResult struct {
	Kind               string             `json:"kind"`
	SchemaVersion      string             `json:"schema_version"`
	KitID              string             `json:"kit_id"`
	URI                string             `json:"uri"`
	File               string             `json:"file"`
	DocumentCID        string             `json:"document_cid"`
	ProtocolCatalogCID string             `json:"protocol_catalog_cid"`
	Entries            []lspDocumentEntry `json:"entries"`
	Diagnostics        []sharedDiagnostic `json:"diagnostics"`
	Statuses           []interface{}      `json:"statuses"`
	Project            interface{}        `json:"project"`
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
	case "analyzeDocument":
		handleAnalyzeDocument(req.ID, req.Params)
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
		Name:               "provekit-lsp-go",
		Version:            "0.1.0",
		ProtocolVersion:    sharedLSPProtocolVersion,
		KitID:              goKitID,
		ProtocolCatalogCID: sharedLSPProtocolCatalogCID,
		Capabilities: lspCapabilities{
			SourceSurfaces:  []string{"go-source"},
			EntryKinds:      []string{"bind-lift-entry", "call-edge"},
			DiagnosticCodes: []string{"provekit.lsp.parse_error", "provekit.lsp.implication_failed"},
			StatusKinds:     []string{"materialize", "emit", "check", "prove"},
		},
	})
}

func handleParse(id interface{}, paramsRaw json.RawMessage) {
	var params parseParams
	if err := json.Unmarshal(paramsRaw, &params); err != nil {
		sendError(id, -32602, "invalid params")
		return
	}

	result, err := buildParseResult(params.Path, params.Source)
	if err != nil {
		sendError(id, -32603, err.Error())
		return
	}
	send(id, result)
}

func handleAnalyzeDocument(id interface{}, paramsRaw json.RawMessage) {
	var params analyzeDocumentParams
	if err := json.Unmarshal(paramsRaw, &params); err != nil {
		sendError(id, -32602, "invalid params")
		return
	}

	path := firstNonEmpty(params.File, params.Path, "source.go")
	source := firstNonEmpty(params.Text, params.Source)
	uri := params.URI
	if uri == "" {
		uri = "file://" + path
	}

	if _, err := parser.ParseFile(token.NewFileSet(), path, source, parser.ParseComments); err != nil {
		send(id, makeAnalysisResult(uri, path, source, nil, []sharedDiagnostic{
			parseErrorDiagnostic(err.Error()),
		}))
		return
	}

	parseResult, err := buildParseResult(path, source)
	if err != nil {
		sendError(id, -32603, err.Error())
		return
	}

	send(id, makeAnalysisResult(
		uri,
		path,
		source,
		analysisEntriesFromParseResult(parseResult, wholeDocumentRange(source)),
		sharedDiagnosticsFromLSP(parseResult.Diagnostics),
	))
}

func buildParseResult(path, source string) (parseResult, error) {
	var decls []ir.Declaration
	warnings := []interface{}{}

	// Walk source for validator structs
	validatorDecls := walkSource(source, path)
	decls = append(decls, validatorDecls...)

	// Scan for //provekit: annotations
	annotationDecls := scanAnnotations(source, path)
	decls = append(decls, annotationDecls...)

	// Marshal declarations
	contractCids := buildContractCids(decls)
	jcs, err := json.Marshal(decls)
	if err != nil {
		return parseResult{}, fmt.Errorf("marshal: %v", err)
	}
	if len(jcs) == 0 || string(jcs) == "null" {
		jcs = []byte("[]")
	}

	// Emit call-edge stream per spec #114 R1.
	// Walk function bodies to find call sites; emit one CallEdgeDeclaration
	// per call site where the calling function has a known contract.
	callEdges := walkCallEdges(source, path, decls)
	edgesJSON, err := json.Marshal(callEdges)
	if err != nil {
		return parseResult{}, fmt.Errorf("marshal call edges: %v", err)
	}
	if len(edgesJSON) == 0 || string(edgesJSON) == "null" {
		edgesJSON = []byte("[]")
	}

	diagnostics := FloorV1SeedForwardPropagator().EmitDiagnostics(LowerFloorSource(source))

	return parseResult{
		Declarations: jcs,
		CallEdges:    edgesJSON,
		Diagnostics:  diagnostics,
		ContractCids: contractCids,
		Warnings:     warnings,
	}, nil
}

func handleShutdown(id interface{}) {
	send(id, nil)
}

func makeAnalysisResult(
	uri string,
	path string,
	source string,
	entries []lspDocumentEntry,
	diagnostics []sharedDiagnostic,
) lspDocumentAnalysisResult {
	if entries == nil {
		entries = []lspDocumentEntry{}
	}
	if diagnostics == nil {
		diagnostics = []sharedDiagnostic{}
	}
	return lspDocumentAnalysisResult{
		Kind:               "lsp-document-analysis",
		SchemaVersion:      "1",
		KitID:              goKitID,
		URI:                uri,
		File:               path,
		DocumentCID:        canonicalizer.ComputeCID([]byte(source)),
		ProtocolCatalogCID: sharedLSPProtocolCatalogCID,
		Entries:            entries,
		Diagnostics:        diagnostics,
		Statuses:           []interface{}{},
		Project:            nil,
	}
}

func analysisEntriesFromParseResult(result parseResult, rng sourceRange) []lspDocumentEntry {
	entries := []lspDocumentEntry{}
	appendRawEntries(&entries, "bind-lift-entry", result.Declarations, rng)
	appendRawEntries(&entries, "call-edge", result.CallEdges, rng)
	return entries
}

func appendRawEntries(entries *[]lspDocumentEntry, kind string, raw json.RawMessage, rng sourceRange) {
	if len(raw) == 0 {
		return
	}
	var values []interface{}
	if err := json.Unmarshal(raw, &values); err != nil {
		return
	}
	for _, value := range values {
		*entries = append(*entries, lspDocumentEntry{
			Kind:  kind,
			Entry: value,
			Range: rng,
		})
	}
}

func sharedDiagnosticsFromLSP(diagnostics []LSPDiagnostic) []sharedDiagnostic {
	shared := make([]sharedDiagnostic, 0, len(diagnostics))
	for _, diagnostic := range diagnostics {
		shared = append(shared, sharedDiagnosticFromLSP(diagnostic))
	}
	return shared
}

func sharedDiagnosticFromLSP(diagnostic LSPDiagnostic) sharedDiagnostic {
	code := diagnostic.Data.Kind
	if code == "" {
		code = diagnostic.Code
	}
	if code == "" {
		code = "provekit.lsp.lift_gap"
	}
	return sharedDiagnostic{
		Code:               code,
		Message:            diagnostic.Message,
		Severity:           sharedSeverity(diagnostic.Severity),
		Range:              sourceRangeFromLSPRange(diagnostic.Range),
		Producer:           "forward-propagation",
		KitID:              goKitID,
		ProtocolCatalogCID: sharedLSPProtocolCatalogCID,
		Data:               diagnostic.Data,
	}
}

func parseErrorDiagnostic(message string) sharedDiagnostic {
	return sharedDiagnostic{
		Code:               "provekit.lsp.parse_error",
		Message:            message,
		Severity:           "error",
		Range:              sourceRange{StartLine: 1, StartCol: 0, EndLine: 1, EndCol: 0},
		Producer:           "kit",
		KitID:              goKitID,
		ProtocolCatalogCID: sharedLSPProtocolCatalogCID,
	}
}

func sharedSeverity(severity int) string {
	switch severity {
	case 1:
		return "error"
	case 2:
		return "warning"
	case 3:
		return "information"
	case 4:
		return "hint"
	default:
		return "information"
	}
}

func sourceRangeFromLSPRange(rng LSPRange) sourceRange {
	return sourceRange{
		StartLine: rng.Start.Line + 1,
		StartCol:  rng.Start.Character,
		EndLine:   rng.End.Line + 1,
		EndCol:    rng.End.Character,
	}
}

func wholeDocumentRange(source string) sourceRange {
	line := 1
	col := 0
	for _, r := range source {
		if r == '\n' {
			line++
			col = 0
			continue
		}
		if r > 0xFFFF {
			col += 2
		} else {
			col++
		}
	}
	return sourceRange{StartLine: 1, StartCol: 0, EndLine: line, EndCol: col}
}

func firstNonEmpty(values ...string) string {
	for _, value := range values {
		if value != "" {
			return value
		}
	}
	return ""
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
	for name, cid := range buildContractCids(decls) {
		idx[name] = cid
	}
	return idx
}

func buildContractCids(decls []ir.Declaration) map[string]string {
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

// CgoPreamble holds the parsed content of a cgo preamble block comment
// (the C code between /* ... */ immediately before `import "C"`).
type CgoPreamble struct {
	// LDFlags contains the combined value of all "#cgo LDFLAGS:" lines,
	// e.g. "-lrustcallee -lz".
	LDFlags string
	// Includes contains each #include path, stripped of angle brackets
	// and quotes, e.g. "rust_callee.h" or "zlib.h".
	Includes []string
}

// parseCgoPreamble scans Go source for the preamble block comment that
// immediately precedes `import "C"`. It is a best-effort line scanner;
// it does not use the Go parser so that it works on source that may not
// parse (e.g. files with build tags). Returns nil if no cgo preamble is found.
func parseCgoPreamble(src string) *CgoPreamble {
	lines := strings.Split(src, "\n")
	// Find the `import "C"` line (also matches `import "C" // comment`).
	importCLine := -1
	for i, l := range lines {
		trimmed := strings.TrimSpace(l)
		if trimmed == `import "C"` || strings.HasPrefix(trimmed, `import "C" `) ||
			strings.HasPrefix(trimmed, "import \"C\"\t") {
			importCLine = i
			break
		}
	}
	if importCLine < 0 {
		return nil
	}

	// Scan upward from importCLine to find the closing */ of the
	// immediately-preceding block comment.
	blockEnd := -1
	for i := importCLine - 1; i >= 0; i-- {
		trimmed := strings.TrimSpace(lines[i])
		if trimmed == "" {
			continue // allow blank lines between */ and import "C"
		}
		if strings.HasSuffix(trimmed, "*/") {
			blockEnd = i
		}
		break
	}
	if blockEnd < 0 {
		return nil
	}

	// Scan upward from blockEnd to find the opening /*, collecting the
	// preamble lines in between.
	blockStart := -1
	for i := blockEnd; i >= 0; i-- {
		trimmed := strings.TrimSpace(lines[i])
		if strings.HasPrefix(trimmed, "/*") {
			blockStart = i
			break
		}
	}
	if blockStart < 0 {
		return nil
	}

	p := &CgoPreamble{}
	for _, l := range lines[blockStart : blockEnd+1] {
		stripped := strings.TrimSpace(l)
		// Strip comment delimiters from the first/last lines.
		stripped = strings.TrimPrefix(stripped, "/*")
		stripped = strings.TrimSuffix(stripped, "*/")
		stripped = strings.TrimPrefix(stripped, "*")
		stripped = strings.TrimSpace(stripped)

		// Parse "#cgo LDFLAGS: ..."
		if after, ok := cutPrefix(stripped, "#cgo LDFLAGS:"); ok {
			p.LDFlags += " " + strings.TrimSpace(after)
			continue
		}
		// Parse "#include <header>" or `#include "header"`
		if after, ok := cutPrefix(stripped, "#include"); ok {
			h := strings.TrimSpace(after)
			h = strings.TrimPrefix(h, "<")
			h = strings.TrimSuffix(h, ">")
			h = strings.TrimPrefix(h, `"`)
			h = strings.TrimSuffix(h, `"`)
			p.Includes = append(p.Includes, h)
		}
	}
	p.LDFlags = strings.TrimSpace(p.LDFlags)
	return p
}

// cutPrefix is a Go 1.18-compatible strings.CutPrefix polyfill.
// Returns (after, true) if s has the prefix p; otherwise ("", false).
func cutPrefix(s, prefix string) (string, bool) {
	if strings.HasPrefix(s, prefix) {
		return s[len(prefix):], true
	}
	return "", false
}

// resolveCgoKit determines which ProvekIt kit a cgo call targets.
//
// Resolution order (first match wins):
//  1. If any included header matches the pattern rust*.h (case-insensitive
//     prefix "rust"), return "rust-kit".
//  2. If LDFlags reference a library where the name starts with "rust"
//     (e.g. -lrustcallee, -lrust_auth), return "rust-kit".
//  3. If LDFlags reference well-known system libraries (-lz, -lm, -lc,
//     -lpthread, -ldl, -lssl, -lcrypto, -lcurl), return "libc-system".
//     These are opaque; the linker won't find a contract for them.
//  4. If LDFlags reference any other explicit -l<lib>, return "c-kit".
//  5. If preamble is nil or has no header/flag signal, return "".
//     The caller must emit a resolver-error edge (spec #97 R2).
//
// Note: the spec's §R3 lists "cgo's C.foo maps to cpp-kit:foo" as one
// example of an FFI convention; the actual kit is preamble-driven here,
// not defaulted, because defaulting silently was what the previous stub
// did and spec §R3 forbids it.
func resolveCgoKit(preamble *CgoPreamble) string {
	if preamble == nil {
		return ""
	}

	// Check headers first (fast signal for the rust+go demo).
	for _, h := range preamble.Includes {
		lower := strings.ToLower(h)
		if strings.HasPrefix(lower, "rust") {
			return "rust-kit"
		}
	}

	// Check LDFLAGS.
	if preamble.LDFlags != "" {
		flags := strings.Fields(preamble.LDFlags)
		// Rust check.
		for _, f := range flags {
			if strings.HasPrefix(f, "-l") {
				lib := strings.TrimPrefix(f, "-l")
				if strings.HasPrefix(strings.ToLower(lib), "rust") {
					return "rust-kit"
				}
			}
		}
		// System libs.
		systemLibs := map[string]bool{
			"z": true, "m": true, "c": true, "pthread": true,
			"dl": true, "ssl": true, "crypto": true, "curl": true,
		}
		for _, f := range flags {
			if strings.HasPrefix(f, "-l") {
				lib := strings.TrimPrefix(f, "-l")
				if systemLibs[lib] {
					return "libc-system"
				}
			}
		}
		// Any other explicit -l → c-kit.
		for _, f := range flags {
			if strings.HasPrefix(f, "-l") {
				return "c-kit"
			}
		}
	}

	return ""
}

// walkCallEdges parses the Go source and, for every function body whose
// function name has a contract in decls, emits one CallEdgeDeclaration
// per call site within that body per spec #114 §1.
//
// Same-kit calls: both sourceContractCid and targetContractCid populated.
// cgo calls (C.<name>(...)): targetContractCid = null, targetSymbol =
// "<kit>:<name>" where kit is resolved from the preamble by resolveCgoKit.
// If resolveCgoKit returns "" (unresolvable), the edge gets
// targetSymbol = "resolver-error:<name>" per spec #97 R2 (fail-loud on
// unresolvable cgo). The linker will promote these to linker-error mementos.
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

	// Parse cgo preamble once for the whole file. All cgo calls in a file
	// share the same preamble; the resolved kit is file-scoped.
	cgoPreamble := parseCgoPreamble(src)
	resolvedCgoKit := resolveCgoKit(cgoPreamble)

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
					if isCgoTypeConversion(targetName) {
						return true
					}
					var sym string
					if resolvedCgoKit != "" {
						sym = resolvedCgoKit + ":" + targetName
					} else {
						// Unresolvable: emit resolver-error prefix so the
						// linker can surface a linker-error memento.
						// Spec #97 R2 forbids silently emitting placeholder strings.
						sym = "resolver-error:" + targetName
					}
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

// isCgoTypeConversion returns true for selectors like C.int(n) and
// C.uint64_t(n). The Go AST represents those conversions as CallExpr nodes,
// but they are argument casts, not cross-kit calls.
func isCgoTypeConversion(name string) bool {
	cgoTypes := map[string]bool{
		"char": true, "schar": true, "uchar": true,
		"short": true, "ushort": true,
		"int": true, "uint": true,
		"long": true, "ulong": true,
		"longlong": true, "ulonglong": true,
		"float": true, "double": true,
		"int8_t": true, "uint8_t": true,
		"int16_t": true, "uint16_t": true,
		"int32_t": true, "uint32_t": true,
		"int64_t": true, "uint64_t": true,
		"intptr_t": true, "uintptr_t": true,
		"size_t": true, "ssize_t": true,
		"bool": true,
	}
	return cgoTypes[name]
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
		// Tag is like `validate:"required,min=1"`: strip outer backticks
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

type paramBinding struct {
	Name string
	Sort ir.Sort
}

type functionSignature struct {
	Name   string
	Params []paramBinding
}

// scanAnnotations scans source lines for //provekit: annotations.
func scanAnnotations(src, path string) []ir.Declaration {
	lines := strings.Split(src, "\n")
	var decls []ir.Declaration
	fset := token.NewFileSet()
	file, _ := parser.ParseFile(fset, path, src, 0)

	for i, line := range lines {
		trimmed := strings.TrimSpace(line)

		if strings.HasPrefix(trimmed, "//provekit:contract") {
			if sig, ok := findAheadFnSignature(fset, file, lines, i); ok {
				decls = append(decls, ir.ContractDeclaration{
					Name:       sig.Name,
					OutBinding: ir.DefaultOutBinding,
					Post:       wrapFormulaForParams(parseContractPost(trimmed), sig.Params),
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

func wrapFormulaForParams(formula ir.IrFormula, params []paramBinding) ir.IrFormula {
	if formula == nil {
		return nil
	}
	for i := len(params) - 1; i >= 0; i-- {
		param := params[i]
		inner := formula
		formula = ir.ForAllNamed(param.Name, param.Sort, func(_ ir.IrTerm) ir.IrFormula {
			return inner
		})
	}
	return formula
}

func parseContractPost(annotation string) ir.IrFormula {
	rest := strings.TrimSpace(strings.TrimPrefix(annotation, "//provekit:contract"))
	if rest == "" {
		return nil
	}
	for _, prefix := range []string{"post=", "post:"} {
		if expr, ok := cutPrefix(rest, prefix); ok {
			return parseSimplePostFormula(strings.TrimSpace(expr))
		}
	}
	return nil
}

func parseSimplePostFormula(expr string) ir.IrFormula {
	expr = strings.ReplaceAll(expr, " ", "")
	if expr == "" {
		return nil
	}
	if strings.Contains(expr, ">=") {
		parts := strings.SplitN(expr, ">=", 2)
		if len(parts) == 2 {
			if value, err := strconv.ParseInt(parts[1], 10, 64); err == nil && parts[0] != "" {
				return ir.Gte(ir.MakeVar(parts[0], ir.Int), ir.Num(value))
			}
		}
		return nil
	}
	parts := strings.SplitN(expr, ">", 2)
	if len(parts) == 2 {
		if value, err := strconv.ParseInt(parts[1], 10, 64); err == nil && parts[0] != "" {
			return ir.Gt(ir.MakeVar(parts[0], ir.Int), ir.Num(value))
		}
	}
	return nil
}

// findAheadFnSignature scans forward from startLine for a Go function
// definition and returns the function's parameter scope.
func findAheadFnSignature(
	fset *token.FileSet,
	file *ast.File,
	lines []string,
	startLine int,
) (functionSignature, bool) {
	const maxLookahead = 10
	start := startLine + 1
	end := start + maxLookahead

	if file != nil {
		for _, decl := range file.Decls {
			fn, ok := decl.(*ast.FuncDecl)
			if !ok || fn.Name == nil {
				continue
			}
			line := fset.Position(fn.Pos()).Line
			if line <= start || line > end+1 {
				continue
			}
			return functionSignature{
				Name:   fn.Name.Name,
				Params: funcParams(fn.Type.Params),
			}, true
		}
	}

	fn := findAheadFn(lines, startLine)
	if fn == "" {
		return functionSignature{}, false
	}
	return functionSignature{Name: fn}, true
}

func funcParams(fields *ast.FieldList) []paramBinding {
	if fields == nil {
		return nil
	}
	var params []paramBinding
	for _, field := range fields.List {
		sort := sortFromASTType(field.Type)
		for _, name := range field.Names {
			if name == nil || name.Name == "" {
				continue
			}
			params = append(params, paramBinding{Name: name.Name, Sort: sort})
		}
	}
	return params
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
