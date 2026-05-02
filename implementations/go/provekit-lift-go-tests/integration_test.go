package lifgotests

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/tsavo/provekit/go/provekit-ir-symbolic/canonicalizer"
	"github.com/tsavo/provekit/go/provekit-ir-symbolic/ir"
)

// TestIntegration_Layer2SampleLiftsAllThreePatterns drives the lift
// over the planted fixture and asserts:
//
//   - Bounded-loop, helper-inlining, and characterization patterns
//     each produce the expected mementos.
//   - The deliberately-skipped nested loop logs a structured warning
//     under the `go-tests-layer2` adapter (NOT `go-tests`, so a
//     report consumer can tell which layer made the call).
//   - The fixture produces ≥ 8 distinct mementos.
//   - Each minted memento round-trips canonical JSON to a stable
//     BLAKE3-512 CID.
func TestIntegration_Layer2SampleLiftsAllThreePatterns(t *testing.T) {
	bytes, err := os.ReadFile(filepath.Join("fixtures", "layer2_sample.go.txt"))
	if err != nil {
		t.Fatalf("read fixture: %v", err)
	}

	out, err := LiftFile(bytes, "fixtures/layer2_sample.go")
	if err != nil {
		t.Fatalf("lift: %v", err)
	}

	if out.Lifted < 8 {
		t.Fatalf("expected ≥8 layer-2 lifts from fixture, got %d (seen=%d, warnings=%d)",
			out.Lifted, out.Seen, len(out.Warnings))
	}

	if len(out.Warnings) == 0 {
		t.Fatalf("expected the nested-loop skip to log a warning")
	}
	nestedWarned := false
	for _, w := range out.Warnings {
		if w.ItemName == "TestNestedLoopSkipped" && strings.Contains(w.Reason, "nested") {
			if w.Adapter != ADAPTER {
				t.Fatalf("expected adapter=%q, got %q", ADAPTER, w.Adapter)
			}
			nestedWarned = true
		}
	}
	if !nestedWarned {
		t.Fatalf("expected nested-loop warning under %q adapter; got %v", ADAPTER, out.Warnings)
	}

	if !out.IsClaimed("TestNestedLoopSkipped") {
		t.Fatalf("nested loop test should be CLAIMED even on skip (so layer 0 doesn't retry)")
	}
}

func TestIntegration_FixtureMintsAtLeastEightDistinctMementos(t *testing.T) {
	bytes, err := os.ReadFile(filepath.Join("fixtures", "layer2_sample.go.txt"))
	if err != nil {
		t.Fatalf("read fixture: %v", err)
	}
	out, err := LiftFile(bytes, "fixtures/layer2_sample.go")
	if err != nil {
		t.Fatalf("lift: %v", err)
	}

	if len(out.Decls) < 8 {
		var names []string
		for _, d := range out.Decls {
			names = append(names, d.Name)
		}
		t.Fatalf("expected ≥8 decls, got %d: %v", len(out.Decls), names)
	}

	// Mint each as a one-shot canonical body and BLAKE3-512 CID.
	// Distinct CIDs == distinct content addresses.
	cids := map[string]bool{}
	for _, d := range out.Decls {
		body, err := ir.MarshalDeclarations([]ir.Declaration{d})
		if err != nil {
			t.Fatalf("marshal %s: %v", d.Name, err)
		}
		cid := canonicalizer.ComputeCID(body)
		if !strings.HasPrefix(cid, "blake3-512:") {
			t.Fatalf("CID missing self-identifying prefix: %s", cid)
		}
		if cids[cid] {
			t.Fatalf("duplicate CID for %q: %s", d.Name, cid)
		}
		cids[cid] = true
	}
	if len(cids) < 8 {
		t.Fatalf("expected ≥8 distinct CIDs, got %d", len(cids))
	}
}

// TestIntegration_CrossKitByteShape pins the exact canonical bytes for
// a single Pattern-1 memento. This is the protocol's byte-equivalence
// guard: any change to predicate name, operand order, key order, or
// JSON escaping in the lift adapter (or the underlying ir package)
// flips the bytes and trips this test before a hash comparison with
// Rust/TS catches it downstream.
//
// Pinned shape: TestSquaresAreNonneg ->
//   forall x:Int. (x ≥ 0 AND x < 100) implies (x ≥ 0)
//
// The unicode predicate names (≥, <) MUST appear verbatim in the
// canonical bytes; any HTML-escaped form (<, >) means the
// JCS path is misconfigured and cross-kit hashes will diverge.
func TestIntegration_CrossKitByteShape(t *testing.T) {
	src := `package x
import "testing"
import "github.com/stretchr/testify/assert"
var _ = assert.Equal
func TestSquaresAreNonneg(t *testing.T) {
	for x := 0; x < 100; x++ {
		assert.True(t, x >= 0)
	}
}
`
	out, err := LiftFile([]byte(src), "t_test.go")
	if err != nil {
		t.Fatalf("lift: %v", err)
	}
	if len(out.Decls) != 1 {
		t.Fatalf("expected 1 decl, got %d", len(out.Decls))
	}
	body, err := ir.MarshalDeclarations([]ir.Declaration{out.Decls[0]})
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	want := `[{"kind":"contract","name":"TestSquaresAreNonneg","outBinding":"out","inv":{"kind":"forall","name":"x","sort":{"kind":"primitive","name":"Int"},"body":{"kind":"implies","operands":[{"kind":"and","operands":[{"kind":"atomic","name":"≥","args":[{"kind":"var","name":"x"},{"kind":"const","value":0,"sort":{"kind":"primitive","name":"Int"}}]},{"kind":"atomic","name":"<","args":[{"kind":"var","name":"x"},{"kind":"const","value":100,"sort":{"kind":"primitive","name":"Int"}}]}]},{"kind":"atomic","name":"≥","args":[{"kind":"var","name":"x"},{"kind":"const","value":0,"sort":{"kind":"primitive","name":"Int"}}]}]}}}]`
	if string(body) != want {
		t.Fatalf("canonical bytes diverged:\n got: %s\nwant: %s", string(body), want)
	}
	// Reject any HTML-escape leak in the canonical bytes. The JSON
	// stdlib's default SetEscapeHTML(true) emits the six-byte escape
	// "&" + "#" + "6" + "0" + ";" for "<" inside string values; the
	// ir package disables that behavior (encodeJSON sets
	// SetEscapeHTML(false)) because cross-kit byte equivalence
	// depends on verbatim emission of the unicode atomic predicates
	// and other bare special characters inside name fields.
	htmlLT := "\\u003c"
	htmlGT := "\\u003e"
	htmlAMP := "\\u0026"
	for _, esc := range []string{htmlLT, htmlGT, htmlAMP} {
		if strings.Contains(string(body), esc) {
			t.Fatalf("canonical bytes contain HTML-escape leak %q; JCS path misconfigured", esc)
		}
	}
}

func TestIntegration_PerPatternSplit(t *testing.T) {
	bytes, err := os.ReadFile(filepath.Join("fixtures", "layer2_sample.go.txt"))
	if err != nil {
		t.Fatalf("read fixture: %v", err)
	}
	out, err := LiftFile(bytes, "fixtures/layer2_sample.go")
	if err != nil {
		t.Fatalf("lift: %v", err)
	}
	t.Logf("LAYER2_SUMMARY: lifted=%d seen=%d warnings=%d "+
		"(loop_lifted=%d loop_skipped=%d helper_lifted=%d helper_skipped=%d char_lifted=%d char_skipped=%d)",
		out.Lifted, out.Seen, len(out.Warnings),
		out.BoundedLoopLifted, out.BoundedLoopSkipped,
		out.HelperInlinedLifted, out.HelperInlinedSkipped,
		out.CharacterizationLifted, out.CharacterizationSkipped)
	// Pattern shape: 3 bounded-loop + 4 helper (3 + 2 = 5? actually 5
	// helper calls, but Pattern 2 emits one per call site) + 1
	// characterization = 9 lifted. The nested loop adds 1 skipped.
	if out.BoundedLoopLifted != 3 {
		t.Errorf("expected bounded_loop_lifted=3, got %d", out.BoundedLoopLifted)
	}
	if out.BoundedLoopSkipped != 1 {
		t.Errorf("expected bounded_loop_skipped=1, got %d", out.BoundedLoopSkipped)
	}
	if out.HelperInlinedLifted != 5 {
		t.Errorf("expected helper_inlined_lifted=5, got %d", out.HelperInlinedLifted)
	}
	if out.CharacterizationLifted != 1 {
		t.Errorf("expected characterization_lifted=1, got %d", out.CharacterizationLifted)
	}
}
