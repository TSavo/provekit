// Command provekit-lift-go-verify is the VERIFY-FACING Go lift surface.
//
// It is the binary the kit-dispatch `go` lift surface resolves for the
// `provekit verify` pipeline. It speaks the legacy-retained
// `initialize` / `lift` / `shutdown` JSON-RPC the language-neutral
// dispatcher drives (implementations/rust/provekit-cli/src/kit_dispatch.rs),
// and returns ONE `ir-document` combining two real Go lift passes over the
// workspace:
//
//  1. Body-derived function-contracts from the library's non-test `.go`
//     files, lifted in the VERIFY-FACING dialect (liftgo.LiftSourceCore):
//     arithmetic / comparison ops are emitted with their SMT-LIB core
//     symbols (`*`, `+`, `<`, ...) so the body-derived
//     `post = result == <body-expr>` is z3-dischargeable. This is the Go
//     analog of Java's ProductionWalk vs JavaSourceLifter split — Go is
//     wired INTO the language-neutral verifier spine by speaking the op
//     vocabulary the spine already understands; the spine is NOT modified.
//
//  2. Harvested callsite assertions from the library's `_test.go` files,
//     via the existing, tested Go Layer-0 assertion harvester
//     (lifgotests.LiftFile): `assert.Equal(t, Double(3), 6)` lifts to a
//     `contract` whose `inv = =(Double(3), 6)` — exactly the harvested
//     `=(<call>, <expected>)` shape the verifier's body-discharge seam
//     enumerates as a callsite.
//
// `provekit mint` then (#1443) auto-writes the `Double -> targetContractCid`
// bridge for the body-bearing function-contract, and `provekit verify`
// reduces `Double(3) == 6` through the body `(* x 2)` -> `(* 3 2) == 6` ->
// z3 discharges (positive) / refutes (broken body, negative).
//
// HONEST: no contract or bridge is hand-written here; both halves are real
// lifter output. Supra omnia, rectum.
package main

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"

	liftgo "github.com/tsavo/provekit/go/provekit-lift-go"
	lifgotests "github.com/tsavo/provekit/go/provekit-lift-go-tests"
)

func main() {
	if len(os.Args) > 1 && os.Args[1] == "--rpc" {
		if err := runRPC(os.Stdin, os.Stdout); err != nil {
			fmt.Fprintf(os.Stderr, "provekit-lift-go-verify rpc: %v\n", err)
			os.Exit(1)
		}
		return
	}
	fmt.Fprintln(os.Stderr, "usage: provekit-lift-go-verify --rpc")
	os.Exit(1)
}

type rpcRequest struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id"`
	Method  string          `json:"method"`
	Params  json.RawMessage `json:"params"`
}

type liftParams struct {
	WorkspaceRoot string   `json:"workspace_root"`
	SourcePaths   []string `json:"source_paths"`
}

func runRPC(stdin io.Reader, stdout io.Writer) error {
	scanner := bufio.NewScanner(stdin)
	scanner.Buffer(make([]byte, 1024*1024), 16*1024*1024)
	for scanner.Scan() {
		line := scanner.Bytes()
		if len(line) == 0 {
			continue
		}
		var req rpcRequest
		if err := json.Unmarshal(line, &req); err != nil {
			writeJSON(stdout, errorResponse(nil, -32700, "PARSE_ERROR"))
			continue
		}
		switch req.Method {
		case "initialize":
			writeJSON(stdout, successResponse(req.ID, map[string]any{
				"name":             "provekit-lift-go-verify",
				"version":          "0.1.0",
				"protocol_version": "pep/1.7.0",
				"capabilities": map[string]any{
					"authoring_surfaces": []string{"go"},
					"ir_version":         liftgo.IRVersion,
				},
			}))
		case "lift":
			writeJSON(stdout, handleLift(req.ID, req.Params))
		case "shutdown":
			writeJSON(stdout, successResponse(req.ID, nil))
			return nil
		default:
			writeJSON(stdout, errorResponse(req.ID, -32601, fmt.Sprintf("METHOD_NOT_FOUND: %s", req.Method)))
		}
	}
	return scanner.Err()
}

func handleLift(id json.RawMessage, raw json.RawMessage) any {
	var params liftParams
	if len(raw) > 0 {
		if err := json.Unmarshal(raw, &params); err != nil {
			return errorResponse(id, -32602, "invalid lift params")
		}
	}
	root := params.WorkspaceRoot
	if root == "" {
		cwd, err := os.Getwd()
		if err == nil {
			root = cwd
		} else {
			root = "."
		}
	}

	irItems, diagnostics, err := liftWorkspace(root)
	if err != nil {
		return errorResponse(id, -32603, fmt.Sprintf("lift failed: %v", err))
	}
	return successResponse(id, map[string]any{
		"kind":        "ir-document",
		"ir":          irItems,
		"diagnostics": diagnostics,
		"refusals":    []any{},
	})
}

// liftWorkspace walks every `.go` file under root, splitting on the
// `_test.go` suffix: non-test files feed the verify-facing body lifter,
// test files feed the assertion harvester. Both halves land in one `ir[]`.
func liftWorkspace(root string) ([]any, []map[string]any, error) {
	var irItems []any
	var diagnostics []map[string]any
	seenFn := map[string]bool{}
	seenContract := map[string]bool{}

	err := filepath.Walk(root, func(path string, info os.FileInfo, walkErr error) error {
		if walkErr != nil {
			return nil
		}
		if info.IsDir() {
			if info.Name() == "vendor" || info.Name() == ".git" || info.Name() == ".provekit" {
				return filepath.SkipDir
			}
			return nil
		}
		if !strings.HasSuffix(path, ".go") {
			return nil
		}
		src, readErr := os.ReadFile(path)
		if readErr != nil {
			return nil
		}

		if strings.HasSuffix(path, "_test.go") {
			// Harvested callsite assertions (Layer-0 leaf harvester): each
			// single top-level `assert.Equal(t, Fn(args), expected)` becomes a
			// `contract` whose `inv = =(Fn(args), expected)` -- the harvested
			// `=(<call>, <expected>)` callsite the verifier reduces through the
			// body-derived function-contract.
			decls, warnings, liftErr := lifgotests.LiftLeafAssertions(src, path)
			if liftErr != nil {
				diagnostics = append(diagnostics, map[string]any{"path": path, "message": liftErr.Error()})
				return nil
			}
			for _, w := range warnings {
				diagnostics = append(diagnostics, map[string]any{"path": w.SourcePath, "message": w.Reason})
			}
			for _, decl := range decls {
				if seenContract[decl.Name] {
					continue
				}
				seenContract[decl.Name] = true
				irItems = append(irItems, decl)
			}
			return nil
		}

		// Body-derived function-contracts (verify-facing dialect).
		rel := relPath(root, path)
		lifted, liftErr := liftgo.LiftSourceCore("", rel, src)
		if liftErr != nil {
			diagnostics = append(diagnostics, map[string]any{"path": path, "message": liftErr.Error()})
			return nil
		}
		for _, d := range lifted.Diagnostics {
			diagnostics = append(diagnostics, map[string]any{"path": d.Path, "message": d.Message})
		}
		for _, fc := range lifted.Contracts {
			if seenFn[fc.FnName] {
				continue
			}
			seenFn[fc.FnName] = true
			item, convErr := functionContractWithBridgeSymbol(fc)
			if convErr != nil {
				diagnostics = append(diagnostics, map[string]any{"path": rel, "message": convErr.Error()})
				continue
			}
			irItems = append(irItems, item)
		}
		return nil
	})
	if err != nil {
		return nil, nil, err
	}
	return irItems, diagnostics, nil
}

// functionContractWithBridgeSymbol serializes a FunctionContract to its
// JSON object form and injects an explicit `bridgeSourceSymbol`: the bare
// function symbol (`Double`) that the harvested call ctor uses and that the
// auto-bridge writer in `provekit mint` (#1443) stamps as the bridge's
// `sourceSymbol`.
//
// This is the protocol's first-class hook for it: `cmd_mint` reads
// `bridgeSourceSymbol` directly (it otherwise derives the symbol from a
// `name` / `symbol` / `fn_name` field, none of which is the lifter's
// `fnName`). Setting it explicitly keeps the round-trip FunctionContract
// shape untouched while making the body-discharge bridge resolve to `Double`,
// so `enumerate_callsites` matches the harvested `=(Double(3), 6)` callsite.
func functionContractWithBridgeSymbol(fc liftgo.FunctionContract) (map[string]any, error) {
	raw, err := json.Marshal(fc)
	if err != nil {
		return nil, fmt.Errorf("marshal function-contract: %w", err)
	}
	var obj map[string]any
	if err := json.Unmarshal(raw, &obj); err != nil {
		return nil, fmt.Errorf("unmarshal function-contract: %w", err)
	}
	obj["bridgeSourceSymbol"] = bareSymbol(fc.FnName)
	return obj, nil
}

// bareSymbol reduces a (possibly package-qualified) function name to the bare
// identifier a harvested call ctor uses: `command-line-arguments.Double` ->
// `Double`. Mirrors the verifier's `simple_function_symbol`.
func bareSymbol(fnName string) string {
	name := fnName
	if i := strings.Index(name, "("); i >= 0 {
		name = name[:i]
	}
	if i := strings.LastIndex(name, "."); i >= 0 {
		name = name[i+1:]
	}
	return name
}

func relPath(root, path string) string {
	rel, err := filepath.Rel(root, path)
	if err != nil {
		return filepath.Base(path)
	}
	return rel
}

func successResponse(id json.RawMessage, result any) map[string]any {
	return map[string]any{"jsonrpc": "2.0", "id": idValue(id), "result": result}
}

func errorResponse(id json.RawMessage, code int, message string) map[string]any {
	return map[string]any{"jsonrpc": "2.0", "id": idValue(id), "error": map[string]any{"code": code, "message": message}}
}

func idValue(id json.RawMessage) any {
	if len(id) == 0 {
		return nil
	}
	var out any
	if err := json.Unmarshal(id, &out); err != nil {
		return nil
	}
	return out
}

func writeJSON(w io.Writer, v any) {
	b, err := json.Marshal(v)
	if err != nil {
		fmt.Fprintf(w, `{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":%q}}`+"\n", err.Error())
		return
	}
	fmt.Fprintln(w, string(b))
}
