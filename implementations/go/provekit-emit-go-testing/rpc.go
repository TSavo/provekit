package emitgotesting

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
)

func Dispatch(request map[string]any) map[string]any {
	id := request["id"]
	method, _ := request["method"].(string)
	params := request["params"]
	if params == nil {
		params = map[string]any{}
	}

	switch method {
	case "provekit.plugin.describe":
		return success(id, PluginMemento())
	case "provekit.plugin.invoke":
		obj, ok := params.(map[string]any)
		if !ok {
			return failure(id, -32602, "INVALID_PARAMS: params must be an object")
		}
		emission := Emit(EmitPlanFromParams(obj))
		return success(id, emissionToMap(emission))
	case "provekit.plugin.shutdown":
		return success(id, nil)
	default:
		return failure(id, -32601, fmt.Sprintf("METHOD_NOT_FOUND: %s", method))
	}
}

func RunRPC(stdin io.Reader, stdout io.Writer) error {
	scanner := bufio.NewScanner(stdin)
	scanner.Buffer(make([]byte, 1024*1024), 16*1024*1024)
	encoder := json.NewEncoder(stdout)
	encoder.SetEscapeHTML(false)
	for scanner.Scan() {
		line := scanner.Bytes()
		if len(line) == 0 {
			continue
		}
		var request map[string]any
		if err := json.Unmarshal(line, &request); err != nil {
			_ = encoder.Encode(failure(nil, -32700, fmt.Sprintf("PARSE_ERROR: %v", err)))
			continue
		}
		response := Dispatch(request)
		if err := encoder.Encode(response); err != nil {
			return err
		}
		if request["method"] == "provekit.plugin.shutdown" {
			return nil
		}
	}
	return scanner.Err()
}

func emissionToMap(emission Emission) map[string]any {
	return map[string]any{
		"kind":                   emission.Kind,
		"source":                 emission.Source,
		"path":                   emission.Path,
		"extension":              emission.Extension,
		"emitted_artifact_cid":   emission.EmittedArtifactCID,
		"emitted_predicates":     stringAnyList(emission.EmittedPredicates),
		"unsupported_predicates": stringAnyList(emission.UnsupportedPredicates),
		"is_complete":            emission.IsComplete,
	}
}

func success(id any, result any) map[string]any {
	return map[string]any{"jsonrpc": "2.0", "id": id, "result": result}
}

func failure(id any, code int, message string) map[string]any {
	return map[string]any{
		"jsonrpc": "2.0",
		"id":      id,
		"error":   map[string]any{"code": code, "message": message},
	}
}
