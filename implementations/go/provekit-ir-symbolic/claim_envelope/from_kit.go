package claim_envelope

import (
	"encoding/json"
	"fmt"

	"github.com/tsavo/provekit/go/provekit-ir-symbolic/ir"
)

// FormulaToValue converts a kit IrFormula to a JSON-shape value tree
// (map[string]interface{}) suitable for embedding in a property
// memento's evidence.body.irFormula. The kit's IR types implement
// MarshalJSON (matching the protocol's IR-JSON encoding); this
// helper round-trips through json.Marshal then json.Unmarshal so the
// downstream JCS encoder gets a plain Go data structure.
func FormulaToValue(f ir.IrFormula) (interface{}, error) {
	bytes, err := json.Marshal(f)
	if err != nil {
		return nil, fmt.Errorf("FormulaToValue: marshal: %w", err)
	}
	var out interface{}
	if err := json.Unmarshal(bytes, &out); err != nil {
		return nil, fmt.Errorf("FormulaToValue: unmarshal: %w", err)
	}
	return out, nil
}
