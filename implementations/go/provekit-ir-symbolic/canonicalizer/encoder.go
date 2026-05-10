// Package canonicalizer implements RFC 8785 JCS-JSON encoding +
// SHA-256 prefix hashes for the protocol's content addressing.
// Spec: protocol/specs/2026-04-30-canonicalization-grammar.md
package canonicalizer

import (
	"encoding/json"
	"fmt"
	"sort"
	"strings"
)

// Encoder is the stateful JCS-JSON encoder. Per RFC 8785 §3:
// UTF-8 output, no whitespace, object keys sorted by Unicode code
// point, minimal string escaping. Reusable across many Encode calls;
// each Encode is independent.
type Encoder struct{}

// NewEncoder returns a fresh JCS encoder. The zero value is also valid.
func NewEncoder() *Encoder {
	return &Encoder{}
}

// Encode produces canonical-JSON bytes for v. Accepts string, bool,
// int / int64, float64, json.Number, []interface{}, map[string]interface{},
// nil. Returns an error on unsupported types or non-finite numbers.
func (e *Encoder) Encode(v interface{}) ([]byte, error) {
	var sb strings.Builder
	if err := e.writeValue(&sb, v); err != nil {
		return nil, err
	}
	return []byte(sb.String()), nil
}

func (e *Encoder) writeValue(sb *strings.Builder, v interface{}) error {
	switch x := v.(type) {
	case nil:
		sb.WriteString("null")
	case bool:
		if x {
			sb.WriteString("true")
		} else {
			sb.WriteString("false")
		}
	case string:
		writeString(sb, x)
	case int:
		fmt.Fprintf(sb, "%d", x)
	case int64:
		fmt.Fprintf(sb, "%d", x)
	case float64:
		// JCS §3.2.2.3: integer-valued doubles render without the decimal.
		if x == float64(int64(x)) {
			fmt.Fprintf(sb, "%d", int64(x))
		} else {
			b, err := json.Marshal(x)
			if err != nil {
				return fmt.Errorf("encode float64: %w", err)
			}
			sb.Write(b)
		}
	case json.Number:
		sb.WriteString(x.String())
	case []interface{}:
		sb.WriteByte('[')
		for i, elem := range x {
			if i > 0 {
				sb.WriteByte(',')
			}
			if err := e.writeValue(sb, elem); err != nil {
				return err
			}
		}
		sb.WriteByte(']')
	case map[string]interface{}:
		keys := make([]string, 0, len(x))
		for k := range x {
			keys = append(keys, k)
		}
		// §7.3: sort by Unicode code-point order. ASCII keys: byte-order suffices.
		sort.Strings(keys)
		sb.WriteByte('{')
		for i, k := range keys {
			if i > 0 {
				sb.WriteByte(',')
			}
			writeString(sb, k)
			sb.WriteByte(':')
			if err := e.writeValue(sb, x[k]); err != nil {
				return err
			}
		}
		sb.WriteByte('}')
	default:
		return fmt.Errorf("Encoder: unsupported type %T", v)
	}
	return nil
}

// writeString applies §7.5 minimal escape: ", \, control chars (U+0000..U+001F).
// All other code points emit verbatim as their UTF-8 bytes.
func writeString(sb *strings.Builder, s string) {
	sb.WriteByte('"')
	for _, c := range []byte(s) {
		switch {
		case c == '"':
			sb.WriteString(`\"`)
		case c == '\\':
			sb.WriteString(`\\`)
		case c < 0x20:
			fmt.Fprintf(sb, `\u%04x`, c)
		default:
			sb.WriteByte(c)
		}
	}
	sb.WriteByte('"')
}

// EncodeJCS is a package-level convenience for one-shot encoding.
func EncodeJCS(v interface{}) ([]byte, error) {
	return NewEncoder().Encode(v)
}
