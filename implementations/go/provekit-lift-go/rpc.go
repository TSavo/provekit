package liftgo

import (
	"bufio"
	"bytes"
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

type liftParams struct {
	WorkspaceRoot string   `json:"workspace_root"`
	SourcePaths   []string `json:"source_paths"`
}

func RunRPC(stdin io.Reader, stdout io.Writer) error {
	scanner := bufio.NewScanner(stdin)
	for scanner.Scan() {
		line := scanner.Bytes()
		if len(line) == 0 {
			continue
		}
		var req rpcRequest
		if err := json.Unmarshal(line, &req); err != nil {
			writeRPC(stdout, errorResponse(nil, -32700, "PARSE_ERROR"))
			continue
		}
		switch req.Method {
		case "initialize":
			writeRPC(stdout, successResponse(req.ID, InitializeResult()))
		case "lift":
			writeRPC(stdout, handleLift(req.ID, req.Params))
		case "compile":
			writeRPC(stdout, handleCompile(req.ID, req.Params))
		case "shutdown":
			writeRPC(stdout, successResponse(req.ID, nil))
			return nil
		default:
			writeRPC(stdout, errorResponse(req.ID, -32601, fmt.Sprintf("METHOD_NOT_FOUND: %s", req.Method)))
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
	if params.WorkspaceRoot == "" {
		cwd, err := os.Getwd()
		if err == nil {
			params.WorkspaceRoot = cwd
		} else {
			params.WorkspaceRoot = "."
		}
	}
	if len(params.SourcePaths) == 0 {
		return errorResponse(id, -32602, "source_paths must be a non-empty array of strings")
	}
	result, err := LiftPaths(params.WorkspaceRoot, params.SourcePaths)
	if err != nil {
		return errorResponse(id, -32603, fmt.Sprintf("Lift failed: %v", err))
	}
	return successResponse(id, map[string]any{
		"kind":          "ir-document",
		"ir":            result.IR,
		"callEdges":     []any{},
		"diagnostics":   result.Diagnostics,
		"opacityReport": []any{},
		"refusals":      result.Refusals,
	})
}

func handleCompile(id json.RawMessage, raw json.RawMessage) any {
	var params struct {
		IR json.RawMessage `json:"ir"`
	}
	if err := json.Unmarshal(raw, &params); err != nil {
		return errorResponse(id, -32602, "invalid compile params")
	}
	irItems, err := sourceUnitIRFromJSON(params.IR)
	if err != nil {
		return errorResponse(id, -32602, "compile params.ir must be an array")
	}
	compiled, err := Compile(CompileInput{IR: irItems})
	if err != nil {
		return errorResponse(id, -32603, fmt.Sprintf("Compile failed: %v", err))
	}
	return successResponse(id, compiled)
}

func successResponse(id json.RawMessage, result any) map[string]any {
	return map[string]any{"jsonrpc": "2.0", "id": idJSONValue(id), "result": result}
}

func errorResponse(id json.RawMessage, code int, message string) map[string]any {
	return map[string]any{"jsonrpc": "2.0", "id": idJSONValue(id), "error": map[string]any{"code": code, "message": message}}
}

func idJSONValue(id json.RawMessage) any {
	if len(id) == 0 {
		return nil
	}
	dec := json.NewDecoder(bytes.NewReader(id))
	dec.UseNumber()
	var out any
	if err := dec.Decode(&out); err != nil {
		return nil
	}
	return out
}

func writeRPC(w io.Writer, v any) {
	b, err := marshalJSONNoHTML(v)
	if err != nil {
		fmt.Fprintf(w, `{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"%s"}}`+"\n", err.Error())
		return
	}
	fmt.Fprintln(w, string(b))
}
