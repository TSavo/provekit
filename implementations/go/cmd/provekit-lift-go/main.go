package main

import (
	"bufio"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/tsavo/provekit/go/provekit-ir-symbolic/canonicalizer"
	"github.com/tsavo/provekit/go/provekit-ir-symbolic/ir"
	lifgotests "github.com/tsavo/provekit/go/provekit-lift-go-tests"
)

func main() {
	if len(os.Args) > 1 && os.Args[1] == "--rpc" {
		runRPCMode()
		return
	}
	dir := "."
	if len(os.Args) > 1 {
		dir = os.Args[1]
	}
	if err := runDirect(dir); err != nil {
		fmt.Fprintf(os.Stderr, "provekit-lift-go: %v\n", err)
		os.Exit(1)
	}
}

func runDirect(dir string) error {
	decls, err := liftDir(dir)
	if err != nil {
		return err
	}
	if len(decls) == 0 {
		return fmt.Errorf("no liftable contracts found in %s", dir)
	}
	body, err := ir.MarshalDeclarations(decls)
	if err != nil {
		return fmt.Errorf("marshal: %w", err)
	}
	cid := canonicalizer.ComputeCID(body)
	outPath := filepath.Join(dir, cid+".proof")
	if err := os.WriteFile(outPath, body, 0644); err != nil {
		return fmt.Errorf("write %s: %w", outPath, err)
	}
	fmt.Printf("provekit-lift-go: lifted %d contracts\n", len(decls))
	fmt.Printf("provekit-lift-go: wrote %s\n", outPath)
	fmt.Printf("provekit-lift-go: cid = %s\n", cid)
	return nil
}

func liftDir(dir string) ([]ir.Declaration, error) {
	var allDecls []ir.Declaration
	seen := map[string]bool{}
	err := filepath.Walk(dir, func(path string, info os.FileInfo, err error) error {
		if err != nil || info.IsDir() {
			if info != nil && (info.Name() == "vendor" || info.Name() == ".git") {
				return filepath.SkipDir
			}
			return nil
		}
		if !strings.HasSuffix(path, "_test.go") {
			return nil
		}
		bytes, err := os.ReadFile(path)
		if err != nil {
			return nil
		}
		out, err := lifgotests.LiftFile(bytes, path)
		if err != nil {
			return nil
		}
		for _, d := range out.Decls {
			if seen[d.Name] {
				continue
			}
			seen[d.Name] = true
			allDecls = append(allDecls, d)
		}
		return nil
	})
	return allDecls, err
}

func runRPCMode() {
	scanner := bufio.NewScanner(os.Stdin)
	for scanner.Scan() {
		line := scanner.Text()
		var req struct {
			JSONRPC string          `json:"jsonrpc"`
			ID      json.RawMessage `json:"id"`
			Method  string          `json:"method"`
		}
		if err := json.Unmarshal([]byte(line), &req); err != nil {
			writeError(nil, -32700, fmt.Sprintf("parse error: %v", err))
			continue
		}
		switch req.Method {
		case "initialize":
			writeResponse(req.ID, map[string]interface{}{
				"name":             "provekit-lift-go",
				"version":          "1.0",
				"protocol_version": "provekit-lift/1",
				"capabilities": map[string]interface{}{
					"authoring_surfaces": []string{"go"},
					"ir_version":         "v1.1.0",
				},
			})
		case "lift":
			workspace, _ := os.Getwd()
			if workspace == "" {
				workspace = "."
			}
			decls, err := liftDir(workspace)
			if err != nil {
				writeError(req.ID, -32603, fmt.Sprintf("lift failed: %v", err))
				continue
			}
			body, err := ir.MarshalDeclarations(decls)
			if err != nil {
				writeError(req.ID, -32603, fmt.Sprintf("marshal failed: %v", err))
				continue
			}
			cid := canonicalizer.ComputeCID(body)
			outPath := filepath.Join(workspace, cid+".proof")
			os.WriteFile(outPath, body, 0644)
			b64 := base64.StdEncoding.EncodeToString(body)
			writeResponse(req.ID, map[string]interface{}{
				"kind":         "proof-envelope",
				"filename_cid": cid,
				"bytes_base64": b64,
			})
		case "shutdown":
			writeResponse(req.ID, nil)
			return
		default:
			writeError(req.ID, -32601, fmt.Sprintf("unknown method: %s", req.Method))
		}
	}
}

func writeResponse(id json.RawMessage, result interface{}) {
	resp := map[string]interface{}{"jsonrpc": "2.0", "id": id, "result": result}
	b, _ := json.Marshal(resp)
	fmt.Println(string(b))
}

func writeError(id json.RawMessage, code int, message string) {
	resp := map[string]interface{}{
		"jsonrpc": "2.0",
		"id":      id,
		"error":   map[string]interface{}{"code": code, "message": message},
	}
	b, _ := json.Marshal(resp)
	fmt.Println(string(b))
}
