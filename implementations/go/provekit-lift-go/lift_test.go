package liftgo

import (
	"bytes"
	"encoding/json"
	"os"
	"reflect"
	"strings"
	"testing"
)

const (
	panicFreedomEffectKind    = "concept:panic-freedom"
	runtimeFailureSiteConcept = "concept:panic-freedom.leaf.runtime-failure-site"
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

func TestBuiltinPanicEmitsRuntimeFailureLocusAndBodyCtor(t *testing.T) {
	src := `package sample

func F() {
panic("boom")
}
`
	result, err := LiftSource("example.com/sample", "panic.go", []byte(src))
	if err != nil {
		t.Fatalf("LiftSource: %v", err)
	}
	contracts := result.FunctionContracts()
	if len(contracts) != 1 {
		t.Fatalf("contracts = %d, refusals=%+v", len(contracts), result.Refusals)
	}
	gotEffects, err := json.Marshal(contracts[0].Effects)
	if err != nil {
		t.Fatalf("marshal effects: %v", err)
	}
	if string(gotEffects) != `[{"kind":"panics"}]` {
		t.Fatalf("panic effect changed:\n got: %s", gotEffects)
	}

	body := sourceUnitBodyTerm(t, result)
	if body["kind"] != "op" || body["name"] != "go:panic" {
		t.Fatalf("explicit panic must lift to go:panic body term, got: %#v", body)
	}
	want := []any{map[string]any{
		"effectKind": panicFreedomEffectKind,
		"callee":     runtimeFailureSiteConcept,
		"subkind":    "explicit-panic",
		"argTerm": map[string]any{
			"kind":  "const",
			"sort":  map[string]any{"kind": "primitive", "name": "String"},
			"value": "boom",
		},
		"file": "panic.go",
		"line": json.Number("4"),
		"col":  json.Number("0"),
	}}
	if got := runtimeFailureLoci(t, contracts[0]); !reflect.DeepEqual(got, want) {
		t.Fatalf("panicLoci mismatch:\n got: %#v\nwant: %#v", got, want)
	}
}

func TestBuiltinPanicRuntimeFailureLocusPreservesIdentifierArg(t *testing.T) {
	src := `package sample

func F(err any) {
panic(err)
}
`
	result, err := LiftSource("example.com/sample", "panic_ident.go", []byte(src))
	if err != nil {
		t.Fatalf("LiftSource: %v", err)
	}
	contracts := result.FunctionContracts()
	if len(contracts) != 1 {
		t.Fatalf("contracts = %d, refusals=%+v", len(contracts), result.Refusals)
	}
	loci := runtimeFailureLoci(t, contracts[0])
	if len(loci) != 1 {
		t.Fatalf("panicLoci = %#v, want one locus", loci)
	}
	locus := loci[0].(map[string]any)
	if locus["subkind"] != "explicit-panic" || locus["line"] != json.Number("4") || locus["col"] != json.Number("0") {
		t.Fatalf("bad identifier panic locus metadata: %#v", locus)
	}
	wantArg := map[string]any{"kind": "var", "name": "err"}
	if !reflect.DeepEqual(locus["argTerm"], wantArg) {
		t.Fatalf("panic argTerm mismatch:\n got: %#v\nwant: %#v", locus["argTerm"], wantArg)
	}
}

func TestBuiltinPanicRuntimeFailureLocusPreservesCallArg(t *testing.T) {
	src := `package sample

import "fmt"

func F() {
panic(fmt.Errorf("boom"))
}
`
	result, err := LiftSource("example.com/sample", "panic_call.go", []byte(src))
	if err != nil {
		t.Fatalf("LiftSource: %v", err)
	}
	contracts := result.FunctionContracts()
	if len(contracts) != 1 {
		t.Fatalf("contracts = %d, refusals=%+v", len(contracts), result.Refusals)
	}
	loci := runtimeFailureLoci(t, contracts[0])
	if len(loci) != 1 {
		t.Fatalf("panicLoci = %#v, want one locus", loci)
	}
	arg, ok := loci[0].(map[string]any)["argTerm"].(map[string]any)
	if !ok {
		t.Fatalf("argTerm = %#v, want object", loci[0])
	}
	if arg["kind"] != "ctor" || arg["name"] != "go:call" {
		t.Fatalf("panic call arg must stay as a go:call ctor, got: %#v", arg)
	}
	args, ok := arg["args"].([]any)
	if !ok || len(args) == 0 {
		t.Fatalf("go:call args = %#v, want nonempty", arg["args"])
	}
	head, ok := args[0].(map[string]any)
	if !ok || head["kind"] != "const" || head["value"] != "fmt.Errorf" {
		t.Fatalf("go:call head = %#v, want fmt.Errorf const", args[0])
	}
}

func TestBuiltinPanicRuntimeFailureLocusPreservesNilArg(t *testing.T) {
	src := `package sample

func F() {
panic(nil)
}
`
	result, err := LiftSource("example.com/sample", "panic_nil.go", []byte(src))
	if err != nil {
		t.Fatalf("LiftSource: %v", err)
	}
	contracts := result.FunctionContracts()
	if len(contracts) != 1 {
		t.Fatalf("contracts = %d, refusals=%+v", len(contracts), result.Refusals)
	}
	loci := runtimeFailureLoci(t, contracts[0])
	if len(loci) != 1 {
		t.Fatalf("panicLoci = %#v, want one locus", loci)
	}
	locus := loci[0].(map[string]any)
	wantArg := map[string]any{"kind": "var", "name": "nil"}
	if !reflect.DeepEqual(locus["argTerm"], wantArg) {
		t.Fatalf("nil panic argTerm mismatch:\n got: %#v\nwant: %#v", locus["argTerm"], wantArg)
	}
}

func TestShadowedPanicDoesNotEmitRuntimeFailureLocus(t *testing.T) {
	src := `package sample

func panic(x any) {}

func F() {
panic("boom")
}
`
	result, err := LiftSource("example.com/sample", "shadow_panic.go", []byte(src))
	if err != nil {
		t.Fatalf("LiftSource: %v", err)
	}
	if len(result.Refusals) != 0 {
		t.Fatalf("unexpected refusals: %+v", result.Refusals)
	}
	for _, contract := range result.FunctionContracts() {
		assertNoRuntimeFailureLoci(t, contract)
	}
}

func TestNonPanicCallsDoNotEmitRuntimeFailureLoci(t *testing.T) {
	src := `package sample

func F(xs []int) int {
return len(xs)
}
`
	result, err := LiftSource("example.com/sample", "len.go", []byte(src))
	if err != nil {
		t.Fatalf("LiftSource: %v", err)
	}
	contracts := result.FunctionContracts()
	if len(contracts) != 1 {
		t.Fatalf("contracts = %d, refusals=%+v", len(contracts), result.Refusals)
	}
	assertNoRuntimeFailureLoci(t, contracts[0])
}

func TestMethodNamedPanicDoesNotEmitRuntimeFailureLocus(t *testing.T) {
	src := `package sample

type T struct{}

func (t T) Panic() {}

func F(t T) {
t.Panic()
}
`
	result, err := LiftSource("example.com/sample", "method_panic.go", []byte(src))
	if err != nil {
		t.Fatalf("LiftSource: %v", err)
	}
	if len(result.Refusals) != 0 {
		t.Fatalf("unexpected refusals: %+v", result.Refusals)
	}
	for _, contract := range result.FunctionContracts() {
		assertNoRuntimeFailureLoci(t, contract)
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
	if result.ProtocolVersion != "provekit-lift/1" {
		t.Fatalf("protocol_version = %q, want provekit-lift/1", result.ProtocolVersion)
	}
	if result.Capabilities.EmitsSignedMementos {
		t.Fatal("source lifter must not claim signed mementos")
	}
	if len(result.Capabilities.AuthoringSurfaces) != 2 ||
		result.Capabilities.AuthoringSurfaces[0] != "go-source" ||
		result.Capabilities.AuthoringSurfaces[1] != "go-implications" {
		t.Fatalf("authoring surfaces = %+v", result.Capabilities.AuthoringSurfaces)
	}
}

func TestRPCProtocolLiftsFixture(t *testing.T) {
	// Drive the RPC loop directly (no subprocess needed: RunRPC accepts io.Reader/io.Writer).
	input := strings.Join([]string{
		`{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}`,
		`{"jsonrpc":"2.0","id":2,"method":"lift","params":{"workspace_root":".","source_paths":["f.go"]}}`,
		`{"jsonrpc":"2.0","id":3,"method":"shutdown"}`,
	}, "\n") + "\n"

	// Write a fixture file so lift has something to scan.
	tmpDir := t.TempDir()
	fixturePath := tmpDir + "/f.go"
	if err := os.WriteFile(fixturePath, []byte(addSource), 0644); err != nil {
		t.Fatalf("write fixture: %v", err)
	}
	orig, err := os.Getwd()
	if err != nil {
		t.Fatalf("getwd: %v", err)
	}
	if err := os.Chdir(tmpDir); err != nil {
		t.Fatalf("chdir: %v", err)
	}
	defer os.Chdir(orig) //nolint:errcheck

	var out bytes.Buffer
	if err := RunRPC(strings.NewReader(input), &out); err != nil {
		t.Fatalf("RunRPC: %v", err)
	}

	lines := strings.Split(strings.TrimSpace(out.String()), "\n")
	if len(lines) < 2 {
		t.Fatalf("expected at least 2 response lines, got %d: %s", len(lines), out.String())
	}

	// Line 0: initialize response.
	var initResp map[string]any
	if err := json.Unmarshal([]byte(lines[0]), &initResp); err != nil {
		t.Fatalf("parse init response: %v", err)
	}
	result, _ := initResp["result"].(map[string]any)
	if result["protocol_version"] != "provekit-lift/1" {
		t.Fatalf("initialize protocol_version = %v, want provekit-lift/1", result["protocol_version"])
	}

	// Line 1: lift response.
	var liftResp map[string]any
	if err := json.Unmarshal([]byte(lines[1]), &liftResp); err != nil {
		t.Fatalf("parse lift response: %v", err)
	}
	liftResult, _ := liftResp["result"].(map[string]any)
	if liftResult == nil {
		t.Fatalf("lift response has no result: %s", lines[1])
	}
	if liftResult["kind"] != "ir-document" {
		t.Fatalf("lift result kind = %v, want ir-document", liftResult["kind"])
	}
	ir, _ := liftResult["ir"].([]any)
	if len(ir) == 0 {
		t.Fatalf("lift result ir is empty")
	}
}

const doubleSource = `package sample

func Double(x int) int {
	return x * 2
}
`

// The round-trip dialect (default) keeps the namespaced op `go:mul`, which the
// Go source compiler round-trips byte-identically.
func TestRoundTripDialectKeepsNamespacedOp(t *testing.T) {
	result, err := LiftSource("example.com/sample", "double.go", []byte(doubleSource))
	if err != nil {
		t.Fatalf("LiftSource: %v", err)
	}
	contracts := result.FunctionContracts()
	if len(contracts) != 1 {
		t.Fatalf("contracts = %d, want 1", len(contracts))
	}
	body, err := json.Marshal(contracts[0].Post)
	if err != nil {
		t.Fatalf("marshal post: %v", err)
	}
	if !strings.Contains(string(body), `"name":"go:mul"`) {
		t.Fatalf("round-trip dialect must emit go:mul, got: %s", body)
	}
	if strings.Contains(string(body), `"name":"*"`) {
		t.Fatalf("round-trip dialect must NOT normalize to core `*`: %s", body)
	}
}

// The verify-facing dialect normalizes arithmetic to the SMT-LIB core symbol
// `*`, so the body-derived postcondition is z3-dischargeable.
func TestCoreDialectNormalizesArithToSmtSymbol(t *testing.T) {
	result, err := LiftSourceCore("example.com/sample", "double.go", []byte(doubleSource))
	if err != nil {
		t.Fatalf("LiftSourceCore: %v", err)
	}
	contracts := result.FunctionContracts()
	if len(contracts) != 1 {
		t.Fatalf("contracts = %d, want 1", len(contracts))
	}
	body, err := json.Marshal(contracts[0].Post)
	if err != nil {
		t.Fatalf("marshal post: %v", err)
	}
	if !strings.Contains(string(body), `"name":"*"`) {
		t.Fatalf("core dialect must emit `*`, got: %s", body)
	}
	// Discrimination: the namespaced form must be gone in the core dialect.
	if strings.Contains(string(body), `"name":"go:mul"`) {
		t.Fatalf("core dialect must NOT leave go:mul: %s", body)
	}
}

// coreArithOp maps only the operators with a faithful Int/Bool core-theory
// counterpart; structurally-unmappable ops (bitwise, shifts, deref) stay
// namespaced so they cannot silently alias to the wrong theory.
func TestCoreArithOpMappingIsBounded(t *testing.T) {
	mapped := map[string]string{
		"go:add": "+", "go:sub": "-", "go:mul": "*",
		"go:eq": "=", "go:lt": "<", "go:le": "<=", "go:gt": ">", "go:ge": ">=",
		"go:and": "and", "go:or": "or", "go:not": "not", "go:neg": "-",
	}
	for in, want := range mapped {
		got, ok := coreArithOp(in)
		if !ok || got != want {
			t.Fatalf("coreArithOp(%q) = (%q, %v), want (%q, true)", in, got, ok, want)
		}
	}
	// Discrimination: ops whose SMT-LIB semantics DIVERGE from Go (or have no
	// faithful core form) must have NO core mapping, so they stay
	// uninterpreted -> Undecidable, never a false discharge.
	//
	// `go:div` / `go:mod` are the cardinal-sin guard (PR #1445): SMT-LIB
	// div/mod floor toward -inf while Go truncates toward zero, so mapping
	// them signed a witness for `Halve(-7) == -4` (false in Go). They MUST
	// stay namespaced until faithful truncation modeling lands.
	for _, unmapped := range []string{
		"go:div", "go:mod", // divergent semantics (truncate vs floor) — cardinal-sin guard
		"go:bitand", "go:bitor", "go:bitxor", "go:bitnot", // no faithful Int core form
		"go:shl", "go:shr", // shifts
		"go:deref", "go:ne", // deref; ne intentionally conservative (left uninterpreted)
	} {
		if got, ok := coreArithOp(unmapped); ok {
			t.Fatalf("coreArithOp(%q) must be unmapped (unfaithful/no core form), got %q", unmapped, got)
		}
	}
}

// The verify-facing dialect must NOT normalize Go integer division to SMT-LIB
// `div`: their rounding diverges on negatives, which would let a Go-false
// equation discharge. The lifted body must keep the uninterpreted `go:div`.
func TestCoreDialectLeavesDivisionUninterpreted(t *testing.T) {
	src := `package sample

func Halve(x int) int {
	return x / 2
}
`
	result, err := LiftSourceCore("example.com/sample", "halve.go", []byte(src))
	if err != nil {
		t.Fatalf("LiftSourceCore: %v", err)
	}
	contracts := result.FunctionContracts()
	if len(contracts) != 1 {
		t.Fatalf("contracts = %d, want 1", len(contracts))
	}
	body, err := json.Marshal(contracts[0].Post)
	if err != nil {
		t.Fatalf("marshal post: %v", err)
	}
	if !strings.Contains(string(body), `"name":"go:div"`) {
		t.Fatalf("division must stay uninterpreted as go:div, got: %s", body)
	}
	// Decisive: it must NOT have been aliased to the floor-division SMT op.
	if strings.Contains(string(body), `"name":"div"`) {
		t.Fatalf("division must NOT normalize to SMT-LIB `div` (floor != Go truncate): %s", body)
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

func runtimeFailureLoci(t *testing.T, contract FunctionContract) []any {
	t.Helper()
	generic := functionContractGeneric(t, contract)
	loci, ok := generic["panicLoci"].([]any)
	if !ok {
		t.Fatalf("panicLoci = %#v, want array", generic["panicLoci"])
	}
	return loci
}

func assertNoRuntimeFailureLoci(t *testing.T, contract FunctionContract) {
	t.Helper()
	generic := functionContractGeneric(t, contract)
	if _, ok := generic["panicLoci"]; ok {
		t.Fatalf("panicLoci must be omitted when empty: %#v", generic["panicLoci"])
	}
}

func functionContractGeneric(t *testing.T, contract FunctionContract) map[string]any {
	t.Helper()
	generic, err := toGeneric(contract)
	if err != nil {
		t.Fatalf("contract to generic: %v", err)
	}
	m, ok := generic.(map[string]any)
	if !ok {
		t.Fatalf("contract generic = %#v, want object", generic)
	}
	return m
}

func sourceUnitBodyTerm(t *testing.T, result LiftResult) map[string]any {
	t.Helper()
	if len(result.SourceUnits) != 1 {
		t.Fatalf("source units = %d, want one", len(result.SourceUnits))
	}
	args, ok := result.SourceUnits[0].Term["args"].([]any)
	if !ok || len(args) != 2 {
		t.Fatalf("source-unit args = %#v, want two args", result.SourceUnits[0].Term["args"])
	}
	body, ok := args[1].(map[string]any)
	if !ok {
		t.Fatalf("source-unit body = %#v, want object", args[1])
	}
	return body
}
