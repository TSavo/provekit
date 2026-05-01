package lifgotests

import (
	"encoding/json"
	"strings"
	"testing"
)

func mustLift(t *testing.T, src string) *Layer2Output {
	t.Helper()
	out, err := LiftFile([]byte(src), "t_test.go")
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	return out
}

const testHeader = `package x

import (
	"testing"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

var _ = assert.Equal
var _ = require.Equal
var _ = testing.T{}
`

func TestPattern1_BoundedLoop_LiftsToForallImplies(t *testing.T) {
	src := testHeader + `
func TestSquaresAreNonneg(t *testing.T) {
	for x := 0; x < 100; x++ {
		assert.True(t, x >= 0)
	}
}
`
	out := mustLift(t, src)
	if out.Lifted != 1 {
		t.Fatalf("expected lifted=1, got %d (warnings=%v)", out.Lifted, out.Warnings)
	}
	if out.BoundedLoopLifted != 1 {
		t.Fatalf("expected bounded_loop_lifted=1, got %d", out.BoundedLoopLifted)
	}
	if !out.IsClaimed("TestSquaresAreNonneg") {
		t.Fatalf("expected TestSquaresAreNonneg claimed")
	}
	// Verify the IR shape: outer = quantifier with name="x".
	b, _ := json.Marshal(out.Decls[0].Inv)
	if !strings.Contains(string(b), `"kind":"forall"`) {
		t.Fatalf("expected forall in IR, got %s", string(b))
	}
	if !strings.Contains(string(b), `"name":"x"`) {
		t.Fatalf("expected loop var name preserved, got %s", string(b))
	}
}

func TestPattern1_InclusiveRange(t *testing.T) {
	src := testHeader + `
func TestInclusive(t *testing.T) {
	for x := 0; x <= 10; x++ {
		assert.True(t, x >= 0)
	}
}
`
	out := mustLift(t, src)
	if out.Lifted != 1 {
		t.Fatalf("expected lifted=1, got %d", out.Lifted)
	}
}

func TestPattern1_SkipsNestedLoopWithWarning(t *testing.T) {
	src := testHeader + `
func TestNested(t *testing.T) {
	for x := 0; x < 10; x++ {
		for y := 0; y < 10; y++ {
			assert.True(t, x >= 0)
		}
	}
}
`
	out := mustLift(t, src)
	if out.Lifted != 0 {
		t.Fatalf("expected lifted=0, got %d", out.Lifted)
	}
	if out.BoundedLoopSkipped != 1 {
		t.Fatalf("expected bounded_loop_skipped=1, got %d", out.BoundedLoopSkipped)
	}
	if len(out.Warnings) == 0 || !strings.Contains(out.Warnings[0].Reason, "nested") {
		t.Fatalf("expected nested-loop warning, got %v", out.Warnings)
	}
	if !out.IsClaimed("TestNested") {
		t.Fatalf("expected TestNested claimed even on skip")
	}
}

func TestPattern2_HelperInlinesEachCall(t *testing.T) {
	src := testHeader + `
func assertIs42(x int64) {
	assert.Equal(nil, x, int64(42))
}
func TestMany42s(t *testing.T) {
	assertIs42(42)
	assertIs42(42)
	assertIs42(42)
}
`
	out := mustLift(t, src)
	if out.Lifted != 3 {
		t.Fatalf("expected lifted=3, got %d (warnings=%v)", out.Lifted, out.Warnings)
	}
	if out.HelperInlinedLifted != 3 {
		t.Fatalf("expected helper_inlined_lifted=3, got %d", out.HelperInlinedLifted)
	}
	names := map[string]bool{}
	for _, d := range out.Decls {
		names[d.Name] = true
	}
	for _, want := range []string{"TestMany42s::call::0", "TestMany42s::call::1", "TestMany42s::call::2"} {
		if !names[want] {
			t.Fatalf("expected memento %q present, got names=%v", want, names)
		}
	}
}

func TestPattern3_CharacterizationLiftsToConjunction(t *testing.T) {
	src := testHeader + `
func TestThreeFacts(t *testing.T) {
	assert.Equal(t, parseInt("0"), 0)
	assert.Equal(t, parseInt("42"), 42)
	assert.NotEqual(t, parseInt("99"), 0)
}
`
	out := mustLift(t, src)
	if out.Lifted != 1 {
		t.Fatalf("expected lifted=1, got %d (warnings=%v)", out.Lifted, out.Warnings)
	}
	if out.CharacterizationLifted != 1 {
		t.Fatalf("expected characterization_lifted=1, got %d", out.CharacterizationLifted)
	}
	b, _ := json.Marshal(out.Decls[0].Inv)
	if !strings.Contains(string(b), `"kind":"and"`) {
		t.Fatalf("expected and-conjunction, got %s", string(b))
	}
}

func TestPattern3_ReleasesClaimWhenOnlyOneAtomLifts(t *testing.T) {
	src := testHeader + `
func TestMixed(t *testing.T) {
	assert.Equal(t, parseInt("0"), 0)
	assert.Equal(t, len([]int{1, 2, 3}), 3)
}
`
	out := mustLift(t, src)
	if out.CharacterizationLifted != 0 {
		t.Fatalf("expected characterization_lifted=0, got %d", out.CharacterizationLifted)
	}
	if out.IsClaimed("TestMixed") {
		t.Fatalf("expected TestMixed UNclaimed (released to layer 0)")
	}
}

func TestNoLayer2PatternMeansNoClaim(t *testing.T) {
	src := testHeader + `
func TestJustOne(t *testing.T) {
	assert.Equal(t, parseInt("0"), 0)
}
`
	out := mustLift(t, src)
	if out.Lifted != 0 {
		t.Fatalf("expected lifted=0, got %d", out.Lifted)
	}
	if len(out.ClaimedTests) != 0 {
		t.Fatalf("expected no claims, got %v", out.ClaimedTests)
	}
}

func TestUnicodeAtomicPredicatesRoundTrip(t *testing.T) {
	src := testHeader + `
func TestComparators(t *testing.T) {
	assert.True(t, x >= 0)
	assert.True(t, y <= 1)
	assert.NotEqual(t, a, b)
}
`
	out := mustLift(t, src)
	if out.Lifted != 1 {
		t.Fatalf("expected lifted=1, got %d (warnings=%v)", out.Lifted, out.Warnings)
	}
	b, _ := json.Marshal(out.Decls[0].Inv)
	// The IR MUST carry the unicode comparators verbatim (no escaping).
	js := string(b)
	for _, want := range []string{"≥", "≤", "≠"} {
		if !strings.Contains(js, want) {
			t.Fatalf("expected verbatim unicode predicate %q in IR, got %s", want, js)
		}
	}
}
