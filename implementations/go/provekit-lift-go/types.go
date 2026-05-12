package liftgo

import (
	"bytes"
	"encoding/json"
	"fmt"
	"sort"

	"github.com/tsavo/provekit/go/provekit-ir-symbolic/canonicalizer"
)

const (
	Version   = "0.1.0-draft"
	IRVersion = "v1.1.0"
)

type Locus struct {
	File *string `json:"file"`
	Line int     `json:"line"`
	Col  int     `json:"col"`
}

type Effect struct {
	Kind    string `json:"kind"`
	Target  string `json:"target,omitempty"`
	Name    string `json:"name,omitempty"`
	LoopCid string `json:"loopCid,omitempty"`
}

type FunctionContract struct {
	AutoMintedMementos []any    `json:"autoMintedMementos"`
	BodyCid            *string  `json:"bodyCid"`
	Effects            []Effect `json:"effects"`
	FnName             string   `json:"fnName"`
	FormalSorts        []any    `json:"formalSorts"`
	Formals            []string `json:"formals"`
	Kind               string   `json:"kind"`
	Locus              Locus    `json:"locus"`
	Post               any      `json:"post"`
	Pre                any      `json:"pre"`
	ReturnSort         any      `json:"returnSort"`
	SchemaVersion      string   `json:"schemaVersion"`
}

type SourceUnit struct {
	Kind          string         `json:"kind"`
	SchemaVersion string         `json:"schemaVersion"`
	Source        string         `json:"source"`
	SourceCid     string         `json:"sourceCid"`
	Signature     string         `json:"signature"`
	Term          map[string]any `json:"term"`
}

type Refusal struct {
	Kind     string `json:"kind"`
	Function string `json:"function,omitempty"`
	Line     int    `json:"line,omitempty"`
	Reason   string `json:"reason"`
}

type Diagnostic struct {
	Path    string `json:"path,omitempty"`
	Message string `json:"message"`
}

type LiftResult struct {
	IR          []any
	Contracts   []FunctionContract
	SourceUnits []SourceUnit
	Refusals    []Refusal
	Diagnostics []Diagnostic
}

func (r LiftResult) FunctionContracts() []FunctionContract {
	out := make([]FunctionContract, len(r.Contracts))
	copy(out, r.Contracts)
	return out
}

type CompileInput struct {
	IR []any `json:"ir"`
}

type CompileOutput struct {
	Source string `json:"source"`
}

type Capabilities struct {
	AuthoringSurfaces   []string `json:"authoring_surfaces"`
	IRVersion           string   `json:"ir_version"`
	EmitsSignedMementos bool     `json:"emits_signed_mementos"`
}

type InitResult struct {
	Name            string       `json:"name"`
	Version         string       `json:"version"`
	ProtocolVersion string       `json:"protocol_version"`
	Capabilities    Capabilities `json:"capabilities"`
}

func InitializeResult() InitResult {
	return InitResult{
		Name:            "provekit-lift-go-source",
		Version:         Version,
		ProtocolVersion: "pep/1.7.0",
		Capabilities: Capabilities{
			AuthoringSurfaces:   []string{"go-source"},
			IRVersion:           IRVersion,
			EmitsSignedMementos: false,
		},
	}
}

func MarshalIR(ir []any) ([]byte, error) {
	generic, err := toGeneric(ir)
	if err != nil {
		return nil, err
	}
	return canonicalizer.EncodeJCS(generic)
}

func canonicalCID(v any) (string, []byte, error) {
	generic, err := toGeneric(v)
	if err != nil {
		return "", nil, err
	}
	bytes, err := canonicalizer.EncodeJCS(generic)
	if err != nil {
		return "", nil, err
	}
	return canonicalizer.ComputeCID(bytes), bytes, nil
}

func toGeneric(v any) (any, error) {
	buf, err := marshalJSONNoHTML(v)
	if err != nil {
		return nil, err
	}
	dec := json.NewDecoder(bytes.NewReader(buf))
	dec.UseNumber()
	var out any
	if err := dec.Decode(&out); err != nil {
		return nil, err
	}
	return out, nil
}

func marshalJSONNoHTML(v any) ([]byte, error) {
	var buf bytes.Buffer
	enc := json.NewEncoder(&buf)
	enc.SetEscapeHTML(false)
	if err := enc.Encode(v); err != nil {
		return nil, err
	}
	out := buf.Bytes()
	if len(out) > 0 && out[len(out)-1] == '\n' {
		out = out[:len(out)-1]
	}
	return out, nil
}

type effectSet struct {
	byKey map[string]Effect
}

func newEffectSet() *effectSet {
	return &effectSet{byKey: map[string]Effect{}}
}

func (s *effectSet) add(e Effect) {
	if e.Kind == "" {
		return
	}
	s.byKey[effectSortKey(e)] = e
}

func (s *effectSet) merge(other *effectSet) {
	for _, e := range other.byKey {
		s.add(e)
	}
}

func (s *effectSet) sorted() []Effect {
	keys := make([]string, 0, len(s.byKey))
	for k := range s.byKey {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	out := make([]Effect, 0, len(keys))
	for _, k := range keys {
		out = append(out, s.byKey[k])
	}
	return out
}

func effectSortKey(e Effect) string {
	switch e.Kind {
	case "reads":
		return "0:reads:" + e.Target
	case "writes":
		return "1:writes:" + e.Target
	case "io":
		return "2:io"
	case "unsafe":
		return "3:unsafe"
	case "panics":
		return "4:panics"
	case "unresolved_call":
		return "5:unresolved:" + e.Name
	case "opaque_loop":
		return "6:opaque_loop:" + e.LoopCid
	default:
		return fmt.Sprintf("9:%s:%s:%s:%s", e.Kind, e.Target, e.Name, e.LoopCid)
	}
}
