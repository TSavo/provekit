package realizego

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"os"
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
		case "provekit.plugin.resolve_dependency_proofs":
			fmt.Fprintln(os.Stderr, "provekit-realize-go-core: resolve_dependency_proofs not yet implemented for go; returning empty proof_paths")
			writeJSON(stdout, successResponse(req.ID, map[string]any{"proof_paths": []string{}}))
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
