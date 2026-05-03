// SPDX-License-Identifier: Apache-2.0
//
// --rpc mode: speak the lift-plugin protocol over NDJSON-on-stdio.
// Spec: protocol/specs/2026-04-30-lift-plugin-protocol.md
//
// Implements the proof-envelope response shape (c): the plugin owns
// the full pipeline (lift + mint + bundle), returns the .proof bytes
// base64-encoded.
package main

import (
	"bufio"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
)

type rpcRequest struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id"`
	Method  string          `json:"method"`
	Params  json.RawMessage `json:"params,omitempty"`
}

type rpcError struct {
	Code    int    `json:"code"`
	Message string `json:"message"`
}

type rpcResponse struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id"`
	Result  interface{}     `json:"result,omitempty"`
	Error   *rpcError       `json:"error,omitempty"`
}

func writeRPC(w *bufio.Writer, resp rpcResponse) {
	resp.JSONRPC = "2.0"
	b, err := json.Marshal(resp)
	if err != nil {
		fmt.Fprintf(os.Stderr, "rpc: marshal response: %v\n", err)
		return
	}
	w.Write(b)
	w.WriteByte('\n')
	w.Flush()
}

func runRPCMode() {
	reader := bufio.NewReader(os.Stdin)
	writer := bufio.NewWriter(os.Stdout)
	for {
		line, err := reader.ReadString('\n')
		if err != nil {
			return // EOF
		}
		var req rpcRequest
		if err := json.Unmarshal([]byte(line), &req); err != nil {
			fmt.Fprintf(os.Stderr, "rpc: parse: %v\n", err)
			continue
		}
		switch req.Method {
		case "initialize":
			writeRPC(writer, rpcResponse{
				ID: req.ID,
				Result: map[string]interface{}{
					"name":             "go-self-contracts",
					"version":          "1.0.0",
					"protocol_version": "provekit-lift/1",
					"capabilities": map[string]interface{}{
						"authoring_surfaces":    []string{"go-self-contracts"},
						"ir_version":            "v1.1.0",
						"emits_signed_mementos": true,
					},
				},
			})

		case "lift":
			tmpDir, err := os.MkdirTemp("", "provekit-go-rpc-*")
			if err != nil {
				writeRPC(writer, rpcResponse{ID: req.ID, Error: &rpcError{
					Code: -32603, Message: fmt.Sprintf("mkdir temp: %v", err)}})
				continue
			}
			result, err := mintSelfProof(tmpDir)
			if err != nil {
				os.RemoveAll(tmpDir)
				writeRPC(writer, rpcResponse{ID: req.ID, Error: &rpcError{
					Code: 1005, Message: fmt.Sprintf("LIFT_FAILED: %v", err)}})
				continue
			}
			bytes, err := os.ReadFile(filepath.Join(tmpDir, result.cid+".proof"))
			if err != nil {
				// Some impls write to result.path directly; try that.
				bytes, err = os.ReadFile(result.path)
				if err != nil {
					os.RemoveAll(tmpDir)
					writeRPC(writer, rpcResponse{ID: req.ID, Error: &rpcError{
						Code: -32603, Message: fmt.Sprintf("read proof: %v", err)}})
					continue
				}
			}
			os.RemoveAll(tmpDir)
			writeRPC(writer, rpcResponse{
				ID: req.ID,
				Result: map[string]interface{}{
					"kind":             "proof-envelope",
					"filename_cid":     result.cid,
					"contract_set_cid": result.contractSetCID,
					"bytes_base64":     base64.StdEncoding.EncodeToString(bytes),
					"diagnostics":      []interface{}{},
				},
			})

		case "shutdown":
			writeRPC(writer, rpcResponse{ID: req.ID, Result: nil})
			return

		default:
			writeRPC(writer, rpcResponse{ID: req.ID, Error: &rpcError{
				Code:    -32601,
				Message: fmt.Sprintf("METHOD_NOT_FOUND: %s", req.Method),
			}})
		}
	}
}
