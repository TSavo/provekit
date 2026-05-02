// Internal helper: round-trip an IrFormula through JSON to a generic
// map shape. Used only by subst.go to walk the formula structure
// without reaching into the ir package's unexported types.

package lifgotests

import (
	"encoding/json"

	"github.com/tsavo/provekit/go/provekit-ir-symbolic/ir"
)

func marshalGeneric(f ir.IrFormula) map[string]any {
	b, err := json.Marshal(f)
	if err != nil {
		return map[string]any{"kind": "atomic", "name": "true", "args": []any{}}
	}
	var out map[string]any
	if err := json.Unmarshal(b, &out); err != nil {
		return map[string]any{"kind": "atomic", "name": "true", "args": []any{}}
	}
	return out
}
