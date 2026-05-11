package liftgo

import (
	"bytes"
	"encoding/json"
	"strings"
	"testing"
)

const addSource = `package sample

func F(x, y int) int {
	return x + y
}
`

func TestLiftSimpleAddEmitsFunctionContractAndSourceUnit(t *testing.T) {
	result, err := LiftSource("example.com/sample", "f.go", []byte(addSource))
	if err != nil {
		t.Fatalf("LiftSource: %v", err)
	}
	if len(result.Refusals) != 0 {
		t.Fatalf("unexpected refusals: %+v", result.Refusals)
	}
	contracts := result.FunctionContracts()
	if len(contracts) != 1 {
		t.Fatalf("contracts = %d, want 1", len(contracts))
	}
	contract := contracts[0]
	if contract.Kind != "function-contract" {
		t.Fatalf("contract kind = %q", contract.Kind)
	}
	if contract.FnName != "example.com/sample.F" {
		t.Fatalf("fnName = %q", contract.FnName)
	}
	if contract.BodyCid == nil || !strings.HasPrefix(*contract.BodyCid, "blake3-512:") {
		t.Fatalf("bodyCid = %v, want blake3-512 prefix", contract.BodyCid)
	}
	if len(contract.Effects) != 0 {
		t.Fatalf("effects = %+v, want empty", contract.Effects)
	}

	body, err := json.Marshal(contract.Post)
	if err != nil {
		t.Fatalf("marshal post: %v", err)
	}
	if !strings.Contains(string(body), `"name":"go:add"`) {
		t.Fatalf("post does not contain go:add: %s", body)
	}

	if len(result.SourceUnits) != 1 {
		t.Fatalf("source units = %d, want 1", len(result.SourceUnits))
	}
	sourceUnit := result.SourceUnits[0]
	if sourceUnit.Term["kind"] != "op" || sourceUnit.Term["name"] != "go:source-unit" {
		t.Fatalf("source unit term = %#v", sourceUnit.Term)
	}
	args, ok := sourceUnit.Term["args"].([]any)
	if !ok || len(args) != 2 {
		t.Fatalf("source unit args = %#v, want 2 args", sourceUnit.Term["args"])
	}
	bytesSlot, ok := args[0].(map[string]any)
	if !ok {
		t.Fatalf("bytes slot = %#v", args[0])
	}
	if bytesSlot["kind"] != "bytes" || bytesSlot["encoding"] != "hex" {
		t.Fatalf("bytes slot = %#v", bytesSlot)
	}
}

func TestRoundTripCompileLiftIsByteIdentical(t *testing.T) {
	first, err := LiftSource("example.com/sample", "f.go", []byte(addSource))
	if err != nil {
		t.Fatalf("first lift: %v", err)
	}
	compiled, err := Compile(CompileInput{IR: first.IR})
	if err != nil {
		t.Fatalf("compile: %v", err)
	}
	second, err := LiftSource("example.com/sample", "f.go", []byte(compiled.Source))
	if err != nil {
		t.Fatalf("second lift: %v", err)
	}
	firstBytes, err := MarshalIR(first.IR)
	if err != nil {
		t.Fatalf("marshal first: %v", err)
	}
	secondBytes, err := MarshalIR(second.IR)
	if err != nil {
		t.Fatalf("marshal second: %v", err)
	}
	if string(firstBytes) != string(secondBytes) {
		t.Fatalf("round-trip IR diverged:\nfirst:  %s\nsecond: %s", firstBytes, secondBytes)
	}
}

func TestRoundTripCompileBareBodyTermIsByteIdentical(t *testing.T) {
	first, err := LiftSource("example.com/sample", "f.go", []byte(addSource))
	if err != nil {
		t.Fatalf("first lift: %v", err)
	}
	contracts := first.FunctionContracts()
	if len(contracts) != 1 {
		t.Fatalf("contracts = %d, want 1", len(contracts))
	}
	originalBody := resultBodyTerm(t, contracts[0].Post)

	compiled, err := Compile(CompileInput{IR: []any{originalBody}})
	if err != nil {
		t.Fatalf("compile bare body: %v", err)
	}

	second, err := LiftSource("example.com/sample", "compiled.go", []byte(compiled.Source))
	if err != nil {
		t.Fatalf("second lift: %v\nsource:\n%s", err, compiled.Source)
	}
	secondContracts := second.FunctionContracts()
	if len(secondContracts) != 1 {
		t.Fatalf("second contracts = %d, refusals=%+v, source:\n%s", len(secondContracts), second.Refusals, compiled.Source)
	}
	roundTrippedBody := resultBodyTerm(t, secondContracts[0].Post)

	firstBytes := canonicalTermBytes(t, originalBody)
	secondBytes := canonicalTermBytes(t, roundTrippedBody)
	if !bytes.Equal(firstBytes, secondBytes) {
		t.Fatalf("round-trip body diverged:\nfirst:  %s\nsecond: %s\nsource:\n%s", firstBytes, secondBytes, compiled.Source)
	}
}

func TestRefusesUnsupportedGoStatementWithoutUnknownOp(t *testing.T) {
	src := `package sample

func F(ch chan int) {
	go func() { ch <- 1 }()
}
`
	result, err := LiftSource("example.com/sample", "bad.go", []byte(src))
	if err != nil {
		t.Fatalf("LiftSource: %v", err)
	}
	if len(result.FunctionContracts()) != 0 {
		t.Fatalf("expected no contracts for unsupported go statement")
	}
	if len(result.Refusals) != 1 {
		t.Fatalf("refusals = %+v, want one", result.Refusals)
	}
	refusal := result.Refusals[0]
	if refusal.Kind == "" || refusal.Function != "example.com/sample.F" || refusal.Line == 0 {
		t.Fatalf("bad refusal shape: %+v", refusal)
	}
	all, err := MarshalIR(result.IR)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	if strings.Contains(string(all), "go:unknown") || strings.Contains(string(all), "go:binop") {
		t.Fatalf("IR leaked catch-all op: %s", all)
	}
}

func TestRefusesMethodsWithUnresolvedReceiverType(t *testing.T) {
	src := `package sample

func (r []int) M() int {
	return 1
}

func (r map[string]int) M() int {
	return 2
}
`
	result, err := LiftSource("example.com/sample", "bad_recv.go", []byte(src))
	if err != nil {
		t.Fatalf("LiftSource: %v", err)
	}
	if contracts := result.FunctionContracts(); len(contracts) != 0 {
		t.Fatalf("contracts = %+v, want none", contracts)
	}
	if len(result.Refusals) != 2 {
		t.Fatalf("refusals = %+v, want two", result.Refusals)
	}
	for _, refusal := range result.Refusals {
		if refusal.Kind != "unresolved-receiver-type" {
			t.Fatalf("refusal kind = %q, want unresolved-receiver-type: %+v", refusal.Kind, result.Refusals)
		}
		if refusal.Function == "" || refusal.Line == 0 {
			t.Fatalf("bad refusal shape: %+v", refusal)
		}
	}
}

func TestEffectsUseCanonicalShapesAndSortOrder(t *testing.T) {
	src := `package sample

var G int

func F(p *int) int {
	G = *p
	panic("boom")
}
`
	result, err := LiftSource("example.com/sample", "effects.go", []byte(src))
	if err != nil {
		t.Fatalf("LiftSource: %v", err)
	}
	contracts := result.FunctionContracts()
	if len(contracts) != 1 {
		t.Fatalf("contracts = %d, refusals=%+v", len(contracts), result.Refusals)
	}
	got, err := json.Marshal(contracts[0].Effects)
	if err != nil {
		t.Fatalf("marshal effects: %v", err)
	}
	want := `[{"kind":"writes","target":"example.com/sample.G"},{"kind":"panics"}]`
	if string(got) != want {
		t.Fatalf("effects mismatch:\n got: %s\nwant: %s", got, want)
	}
}

func TestInitializeReportsDraftVersionAndCapabilities(t *testing.T) {
	result := InitializeResult()
	if result.Version != Version {
		t.Fatalf("version = %q, want %q", result.Version, Version)
	}
	if result.Version != "0.1.0-draft" {
		t.Fatalf("version must stay draft, got %q", result.Version)
	}
	if result.Capabilities.EmitsSignedMementos {
		t.Fatal("source lifter must not claim signed mementos")
	}
	if len(result.Capabilities.AuthoringSurfaces) != 1 || result.Capabilities.AuthoringSurfaces[0] != "go-source" {
		t.Fatalf("authoring surfaces = %+v", result.Capabilities.AuthoringSurfaces)
	}
}

func resultBodyTerm(t *testing.T, post any) any {
	t.Helper()
	generic, err := toGeneric(post)
	if err != nil {
		t.Fatalf("post to generic: %v", err)
	}
	m, ok := generic.(map[string]any)
	if !ok || m["kind"] != "atomic" || m["name"] != "=" {
		t.Fatalf("post = %#v, want result equality", generic)
	}
	args, ok := m["args"].([]any)
	if !ok || len(args) != 2 {
		t.Fatalf("post args = %#v, want two", m["args"])
	}
	return args[1]
}

func canonicalTermBytes(t *testing.T, term any) []byte {
	t.Helper()
	_, bytes, err := canonicalCID(term)
	if err != nil {
		t.Fatalf("canonical term: %v", err)
	}
	return bytes
}
