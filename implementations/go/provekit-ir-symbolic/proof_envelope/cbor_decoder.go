package proof_envelope

import (
	"encoding/binary"
	"fmt"
)

// CBORDecoder reads deterministic CBOR back into a map for inspection
// (e.g. when verifying or walking a .proof file). Subset matching the
// encoder: uint, tstr, bstr, array, map.
type CBORDecoder struct {
	buf []byte
	pos int
}

// NewCBORDecoder wraps `bytes` for parsing.
func NewCBORDecoder(bytes []byte) *CBORDecoder {
	return &CBORDecoder{buf: bytes}
}

// DecodeCatalog reads a .proof file's top-level map and returns its
// fields as a map[string]interface{}. Members map decodes to
// map[string][]byte (cid → envelope bytes); other map values decode
// to string or []byte as appropriate.
func (d *CBORDecoder) DecodeCatalog() (map[string]interface{}, error) {
	v, err := d.readValue()
	if err != nil {
		return nil, err
	}
	asMap, ok := v.(map[string]interface{})
	if !ok {
		return nil, fmt.Errorf("DecodeCatalog: top-level item is not a map (got %T)", v)
	}
	return asMap, nil
}

// readHead returns (major, arg, error). Implements the inverse of
// CBOREncoder.AppendHead per RFC 8949 §3.
func (d *CBORDecoder) readHead() (CborMajor, uint64, error) {
	if d.pos >= len(d.buf) {
		return 0, 0, fmt.Errorf("readHead: unexpected EOF")
	}
	first := d.buf[d.pos]
	d.pos++
	major := CborMajor(first >> 5)
	info := first & 0x1F
	var arg uint64
	switch {
	case info < 24:
		arg = uint64(info)
	case info == 24:
		if d.pos+1 > len(d.buf) {
			return 0, 0, fmt.Errorf("readHead: truncated u8")
		}
		arg = uint64(d.buf[d.pos])
		d.pos++
	case info == 25:
		if d.pos+2 > len(d.buf) {
			return 0, 0, fmt.Errorf("readHead: truncated u16")
		}
		arg = uint64(binary.BigEndian.Uint16(d.buf[d.pos:]))
		d.pos += 2
	case info == 26:
		if d.pos+4 > len(d.buf) {
			return 0, 0, fmt.Errorf("readHead: truncated u32")
		}
		arg = uint64(binary.BigEndian.Uint32(d.buf[d.pos:]))
		d.pos += 4
	case info == 27:
		if d.pos+8 > len(d.buf) {
			return 0, 0, fmt.Errorf("readHead: truncated u64")
		}
		arg = binary.BigEndian.Uint64(d.buf[d.pos:])
		d.pos += 8
	default:
		return 0, 0, fmt.Errorf("readHead: indefinite-length items not supported (info=%d)", info)
	}
	return major, arg, nil
}

func (d *CBORDecoder) readValue() (interface{}, error) {
	major, arg, err := d.readHead()
	if err != nil {
		return nil, err
	}
	switch major {
	case MajorUInt:
		return arg, nil
	case MajorBStr:
		if d.pos+int(arg) > len(d.buf) {
			return nil, fmt.Errorf("readValue: bstr length %d exceeds remaining %d", arg, len(d.buf)-d.pos)
		}
		out := make([]byte, arg)
		copy(out, d.buf[d.pos:d.pos+int(arg)])
		d.pos += int(arg)
		return out, nil
	case MajorTStr:
		if d.pos+int(arg) > len(d.buf) {
			return nil, fmt.Errorf("readValue: tstr length %d exceeds remaining %d", arg, len(d.buf)-d.pos)
		}
		s := string(d.buf[d.pos : d.pos+int(arg)])
		d.pos += int(arg)
		return s, nil
	case MajorArray:
		out := make([]interface{}, arg)
		for i := uint64(0); i < arg; i++ {
			v, err := d.readValue()
			if err != nil {
				return nil, err
			}
			out[i] = v
		}
		return out, nil
	case MajorMap:
		out := make(map[string]interface{}, arg)
		for i := uint64(0); i < arg; i++ {
			k, err := d.readValue()
			if err != nil {
				return nil, err
			}
			ks, ok := k.(string)
			if !ok {
				return nil, fmt.Errorf("readValue: map key is not tstr (got %T)", k)
			}
			v, err := d.readValue()
			if err != nil {
				return nil, err
			}
			out[ks] = v
		}
		return out, nil
	default:
		return nil, fmt.Errorf("readValue: unsupported major type %d", major)
	}
}
