package realizego

import (
	"bufio"
	"bytes"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

type rpcRequest struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id"`
	Method  string          `json:"method"`
	Params  json.RawMessage `json:"params"`
}

// RunRPC drives the PEP 1.7.0 realize protocol the dispatcher
// (kit_dispatch.rs invoke_realize) speaks: a single `provekit.plugin.invoke`
// line in, one JSON result line out. Also answers `provekit.plugin.shutdown`.
func RunRPC(stdin io.Reader, stdout io.Writer) error {
	scanner := bufio.NewScanner(stdin)
	scanner.Buffer(make([]byte, 1024*1024), 16*1024*1024)
	for scanner.Scan() {
		line := scanner.Bytes()
		if len(line) == 0 {
			continue
		}
		var req rpcRequest
		if err := json.Unmarshal(line, &req); err != nil {
			writeJSON(stdout, errorResponse(nil, -32700, fmt.Sprintf("PARSE_ERROR: %v", err)))
			continue
		}
		switch req.Method {
		case "provekit.plugin.invoke":
			writeJSON(stdout, handleInvoke(req.ID, req.Params))
		case "provekit.plugin.assemble":
			writeJSON(stdout, handleAssemble(req.ID, req.Params))
		case "provekit.plugin.materialize_source":
			writeJSON(stdout, handleMaterializeSource(req.ID, req.Params))
		case "provekit.plugin.resolve_dependency_proofs":
			writeJSON(stdout, handleResolveDependencyProofs(req.ID, req.Params))
		case "provekit.plugin.body_template_entries":
			writeJSON(stdout, handleBodyTemplateEntries(req.ID, req.Params))
		case "provekit.plugin.check":
			writeJSON(stdout, handleCheck(req.ID, req.Params))
		case "provekit.plugin.shutdown":
			writeJSON(stdout, successResponse(req.ID, nil))
			return nil
		default:
			writeJSON(stdout, errorResponse(req.ID, -32601, fmt.Sprintf("METHOD_NOT_FOUND: %s", req.Method)))
		}
	}
	return scanner.Err()
}

func handleInvoke(id json.RawMessage, raw json.RawMessage) any {
	var req RealizeRequest
	if len(raw) > 0 {
		if err := json.Unmarshal(raw, &req); err != nil {
			return errorResponse(id, -32602, fmt.Sprintf("INVALID_PARAMS: %v", err))
		}
	}
	realized, err := Realize(req)
	if err != nil {
		var missing *MissingTemplateError
		if asMissing(err, &missing) {
			// Substrate-honest refusal: the concept is not covered.
			return map[string]any{
				"jsonrpc": "2.0",
				"id":      idValue(id),
				"error": map[string]any{
					"code":    -32100,
					"message": "missing body-template entry",
					"data":    map[string]any{"concept_name": missing.ConceptName, "num_params": missing.NumParams},
				},
			}
		}
		return errorResponse(id, -32603, err.Error())
	}
	return successResponse(id, realized)
}

func handleBodyTemplateEntries(id json.RawMessage, raw json.RawMessage) any {
	var params map[string]any
	if len(raw) > 0 {
		if err := json.Unmarshal(raw, &params); err != nil {
			return errorResponse(id, -32602, fmt.Sprintf("INVALID_PARAMS: %v", err))
		}
	}
	if params == nil {
		params = map[string]any{}
	}
	entries, err := loadBodyTemplateEntriesForProject(
		projectRootFromParams(params),
		libraryTagFromParams(params),
	)
	if err != nil {
		return errorResponse(id, -32031, "BODY_TEMPLATE_ENTRIES_FAILED: "+err.Error())
	}
	return successResponse(id, map[string]any{"entries": entries})
}

func handleResolveDependencyProofs(id json.RawMessage, raw json.RawMessage) any {
	var params map[string]any
	if len(raw) > 0 {
		if err := json.Unmarshal(raw, &params); err != nil {
			return errorResponse(id, -32602, fmt.Sprintf("INVALID_PARAMS: %v", err))
		}
	}
	if params == nil {
		params = map[string]any{}
	}
	proofs, err := resolveDependencyProofs(projectRootFromParams(params))
	if err != nil {
		return errorResponse(id, -32030, "RESOLVE_DEPENDENCY_PROOFS_FAILED: "+err.Error())
	}
	return successResponse(id, map[string]any{"proofs": proofs})
}

func handleCheck(id json.RawMessage, raw json.RawMessage) any {
	var params map[string]any
	if len(raw) > 0 {
		if err := json.Unmarshal(raw, &params); err != nil {
			return errorResponse(id, -32602, fmt.Sprintf("INVALID_PARAMS: %v", err))
		}
	}
	if params == nil {
		params = map[string]any{}
	}
	outDir, _ := params["out_dir"].(string)
	if outDir == "" {
		return successResponse(id, map[string]any{"ok": false, "command": "go test ./...", "stderr": "missing out_dir"})
	}
	cmd := exec.Command("go", "test", "./...")
	cmd.Dir = filepath.Clean(outDir)
	output, err := cmd.CombinedOutput()
	report := map[string]any{
		"ok":      err == nil,
		"command": "go test ./...",
		"stderr":  string(output),
	}
	if err != nil {
		report["error"] = err.Error()
	}
	return successResponse(id, report)
}

type listedGoModule struct {
	Path    string          `json:"Path"`
	Dir     string          `json:"Dir"`
	Main    bool            `json:"Main"`
	Replace *listedGoModule `json:"Replace"`
}

type dependencyProof struct {
	CID         string `json:"cid"`
	BytesBase64 string `json:"bytes_base64"`
	Source      string `json:"source"`
}

var dependencyProofName = regexp.MustCompile(`^blake3-512:[0-9a-f]{128}\.proof$`)

func resolveDependencyProofs(projectRoot string) ([]dependencyProof, error) {
	paths, err := resolveDependencyProofPaths(projectRoot)
	if err != nil {
		return nil, err
	}
	proofs := make([]dependencyProof, 0, len(paths))
	for _, path := range paths {
		bytes, err := os.ReadFile(path)
		if err != nil {
			return nil, err
		}
		proofs = append(proofs, dependencyProof{
			CID:         strings.TrimSuffix(filepath.Base(path), ".proof"),
			BytesBase64: base64.StdEncoding.EncodeToString(bytes),
			Source:      "go-module:" + filepath.Base(path),
		})
	}
	return proofs, nil
}

func resolveDependencyProofPaths(projectRoot string) ([]string, error) {
	root := strings.TrimSpace(projectRoot)
	if root == "" {
		wd, err := os.Getwd()
		if err != nil {
			return nil, err
		}
		root = wd
	}
	absRoot, err := filepath.Abs(root)
	if err != nil {
		return nil, err
	}
	absRoot = filepath.Clean(absRoot)
	if _, err := os.Stat(filepath.Join(absRoot, "go.mod")); err != nil {
		if os.IsNotExist(err) {
			return []string{}, nil
		}
		return nil, err
	}

	cmd := exec.Command("go", "list", "-m", "-json", "all")
	cmd.Dir = absRoot
	output, err := cmd.CombinedOutput()
	if err != nil {
		detail := strings.TrimSpace(string(output))
		if detail != "" {
			return nil, fmt.Errorf("go list -m -json all failed: %w: %s", err, detail)
		}
		return nil, fmt.Errorf("go list -m -json all failed: %w", err)
	}

	proofs := map[string]struct{}{}
	decoder := json.NewDecoder(bytes.NewReader(output))
	for {
		var module listedGoModule
		if err := decoder.Decode(&module); err != nil {
			if err == io.EOF {
				break
			}
			return nil, fmt.Errorf("decode go list module: %w", err)
		}
		if module.Main {
			continue
		}
		dir := effectiveModuleDir(module)
		if dir == "" {
			continue
		}
		if !filepath.IsAbs(dir) {
			dir = filepath.Join(absRoot, dir)
		}
		if err := collectProofPaths(filepath.Clean(dir), proofs); err != nil {
			return nil, err
		}
	}

	originals := make([]string, 0, len(proofs))
	for proof := range proofs {
		originals = append(originals, proof)
	}
	sort.Strings(originals)
	return originals, nil
}

func effectiveModuleDir(module listedGoModule) string {
	if module.Dir != "" {
		return module.Dir
	}
	if module.Replace != nil {
		return effectiveModuleDir(*module.Replace)
	}
	return ""
}

func collectProofPaths(root string, proofs map[string]struct{}) error {
	info, err := os.Stat(root)
	if err != nil {
		if os.IsNotExist(err) {
			return nil
		}
		return err
	}
	if !info.IsDir() {
		return nil
	}
	return filepath.WalkDir(root, func(path string, entry os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if entry.IsDir() {
			switch entry.Name() {
			case ".git", "vendor":
				if path != root {
					return filepath.SkipDir
				}
			}
			return nil
		}
		if !dependencyProofName.MatchString(entry.Name()) {
			return nil
		}
		abs, err := filepath.Abs(path)
		if err != nil {
			return err
		}
		proofs[filepath.Clean(abs)] = struct{}{}
		return nil
	})
}

func asMissing(err error, target **MissingTemplateError) bool {
	if m, ok := err.(*MissingTemplateError); ok {
		*target = m
		return true
	}
	return false
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
