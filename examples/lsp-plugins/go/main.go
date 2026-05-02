// ProvekIt LSP Language Plugin — Go
//
// A standalone binary that speaks provekit-lsp-plugin/1 over stdio.
// Parses Go source files and extracts provekit annotations.
//
// Usage: go run main.go --rpc
//
// To use this plugin, add to `.provekit/config.toml`:
//   [[language]]
//   name = "go"
//   extensions = [".go"]
//   plugin = "provekit-lsp-go"
//
// Build: go build -o provekit-lsp-go main.go

package main

import (
	"bufio"
	"encoding/json"
	"fmt"
	"os"
	"regexp"
	"strings"
)

type Request struct {
	Jsonrpc string          `json:"jsonrpc"`
	ID      interface{}     `json:"id"`
	Method  string          `json:"method"`
	Params  json.RawMessage `json:"params"`
}

type Response struct {
	Jsonrpc string      `json:"jsonrpc"`
	ID      interface{} `json:"id"`
	Result  interface{} `json:"result,omitempty"`
	Error   *RpcError   `json:"error,omitempty"`
}

type RpcError struct {
	Code    int    `json:"code"`
	Message string `json:"message"`
}

type ParseParams struct {
	URI  string `json:"uri"`
	Text string `json:"text"`
}

type Annotation struct {
	FunctionName string  `json:"function_name"`
	Kind         string  `json:"kind"`
	TargetCID    *string `json:"target_cid,omitempty"`
	Range        Range   `json:"range"`
}

type Range struct {
	Start Position `json:"start"`
	End   Position `json:"end"`
}

type Position struct {
	Line      uint32 `json:"line"`
	Character uint32 `json:"character"`
}

func main() {
	var rpcMode bool
	for _, arg := range os.Args[1:] {
		if arg == "--rpc" {
			rpcMode = true
		}
	}
	if !rpcMode {
		fmt.Fprintln(os.Stderr, "Usage: provekit-lsp-go --rpc")
		os.Exit(1)
	}

	reImpl := regexp.MustCompile(`//provekit:implement\s+([\w-]+)`)
	reContract := regexp.MustCompile(`//provekit:contract`)
	reVerify := regexp.MustCompile(`//provekit:verify`)
	reFn := regexp.MustCompile(`^func\s+(?:\([^)]+\)\s+)?(\w+)`)

	scanner := bufio.NewScanner(os.Stdin)
	writer := bufio.NewWriter(os.Stdout)

	for scanner.Scan() {
		line := scanner.Text()
		var req Request
		if err := json.Unmarshal([]byte(line), &req); err != nil {
			resp := Response{Jsonrpc: "2.0", ID: nil, Error: &RpcError{Code: -32700, Message: "parse error: " + err.Error()}}
			writeResp(writer, resp)
			continue
		}

		switch req.Method {
		case "initialize":
			resp := Response{
				Jsonrpc: "2.0",
				ID:      req.ID,
				Result: map[string]interface{}{
					"name":         "provekit-lsp-go",
					"version":      "0.1.0",
					"capabilities": []string{},
				},
			}
			writeResp(writer, resp)

		case "parse":
			var params ParseParams
			json.Unmarshal(req.Params, &params)
			annotations := parseGo(params.Text, reImpl, reContract, reVerify, reFn)
			resp := Response{
				Jsonrpc: "2.0",
				ID:      req.ID,
				Result:  map[string]interface{}{"annotations": annotations},
			}
			writeResp(writer, resp)

		case "shutdown":
			resp := Response{Jsonrpc: "2.0", ID: req.ID, Result: nil}
			writeResp(writer, resp)
			return

		default:
			resp := Response{
				Jsonrpc: "2.0",
				ID:      req.ID,
				Error:   &RpcError{Code: -32601, Message: "unknown method: " + req.Method},
			}
			writeResp(writer, resp)
		}
	}
}

func writeResp(w *bufio.Writer, resp Response) {
	b, _ := json.Marshal(resp)
	w.WriteString(string(b))
	w.WriteByte('\n')
	w.Flush()
}

func parseGo(text string, reImpl, reContract, reVerify, reFn *regexp.Regexp) []Annotation {
	var annotations []Annotation
	lines := strings.Split(text, "\n")

	for i, line := range lines {
		lineNum := uint32(i)

		if matches := reImpl.FindStringSubmatch(line); len(matches) > 1 {
			cid := matches[1]
			fnName := findAhead(lines, i, reFn)
			annotations = append(annotations, Annotation{
				FunctionName: fnName,
				Kind:         "implement",
				TargetCID:    &cid,
				Range: Range{
					Start: Position{Line: lineNum, Character: 0},
					End:   Position{Line: lineNum + 1, Character: 0},
				},
			})
		}

		if reContract.MatchString(line) {
			fnName := findAhead(lines, i, reFn)
			annotations = append(annotations, Annotation{
				FunctionName: fnName,
				Kind:         "contract",
				Range: Range{
					Start: Position{Line: lineNum, Character: 0},
					End:   Position{Line: lineNum + 1, Character: 0},
				},
			})
		}

		if reVerify.MatchString(line) {
			fnName := findAhead(lines, i, reFn)
			annotations = append(annotations, Annotation{
				FunctionName: fnName,
				Kind:         "verify",
				Range: Range{
					Start: Position{Line: lineNum, Character: 0},
					End:   Position{Line: lineNum + 1, Character: 0},
				},
			})
		}
	}

	return annotations
}

func findAhead(lines []string, start int, re *regexp.Regexp) string {
	for j := start + 1; j < len(lines) && j < start+10; j++ {
		if matches := re.FindStringSubmatch(lines[j]); len(matches) > 1 {
			return matches[1]
		}
	}
	return "unknown"
}
