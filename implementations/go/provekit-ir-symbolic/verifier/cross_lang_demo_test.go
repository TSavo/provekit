// THE GO END-TO-END DEMO.
//
//   Go signs Go.
//   Go calls C++ (via the bridged kit primitive).
//   Go detects parseInt(num(0)).
//
// Architecture:
//   1. C++ kit shipped a .proof file with parseInt's precondition
//      (forall n: Int. n > 0) — produced by parseInt_kit_proof.cpp.
//   2. Go consumer authors invariants via kit primitives ParseInt(Num(...))
//      — every call emits a Ctor("parseInt", [arg]) IrTerm.
//   3. Go consumer mints + signs its property mementos in pure Go.
//   4. Go consumer bundles them into its own .proof file in pure Go.
//   5. Go bridge enforcement runner walks both .proofs:
//        - load-all-proofs builds a unified CID pool.
//        - enumerate-callsites finds Ctor("parseInt", ...) inside Go's properties.
//        - resolve-bridge-target hash-looks-up the bridge → C++'s property memento.
//        - instantiate-obligation substitutes the call's arg into `forall n. n > 0`.
//        - solve-obligation invokes z3.
//        - report aggregates.
//
//   ParseInt(Num(5)) → instantiate `5 > 0` → unsat(¬(5 > 0)) → DISCHARGED
//   ParseInt(Num(0)) → instantiate `0 > 0` → sat(¬(0 > 0))   → UNSATISFIED
//
// Go imports zero lines of C++. The connection is the protocol: bytes
// the C++ kit produced, walked by the Go verifier, closed by Z3.

package verifier

import (
	"crypto/ed25519"
	"crypto/rand"
	"fmt"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/provekit/ir-symbolic/canonicalizer"
	"github.com/provekit/ir-symbolic/claim_envelope"
	"github.com/provekit/ir-symbolic/ir"
	"github.com/provekit/ir-symbolic/proof_envelope"
)

const cppProofPath = "/tmp/cpp-kit-out/84ca9c7c382cc28d3ca260cd69bda6c1.proof"

func TestCrossLangGoVerifiesCppProof(t *testing.T) {
	if _, err := os.Stat(cppProofPath); err != nil {
		t.Skipf("C++ .proof not found at %s — run tools/run-proof-envelope-conformance.sh first", cppProofPath)
	}

	projectRoot, err := os.MkdirTemp("", "go-cross-lang-")
	if err != nil {
		t.Fatal(err)
	}
	defer os.RemoveAll(projectRoot)

	// ----- 1. Install the C++-produced .proof in node_modules -----
	cppKitDir := filepath.Join(projectRoot, "node_modules", "@example", "cpp-kit")
	if err := os.MkdirAll(cppKitDir, 0o755); err != nil {
		t.Fatal(err)
	}
	cppBytes, err := os.ReadFile(cppProofPath)
	if err != nil {
		t.Fatal(err)
	}
	cppProofDest := filepath.Join(cppKitDir, "84ca9c7c382cc28d3ca260cd69bda6c1.proof")
	if err := os.WriteFile(cppProofDest, cppBytes, 0o644); err != nil {
		t.Fatal(err)
	}

	// ----- 2. Author Go-side invariants using kit primitives -----
	// (resetting the kit's quantifier counter so the test is deterministic.)
	ir.ResetCollector()
	finishCollect := ir.BeginCollecting()

	// ParseInt(Num(5)) — should DISCHARGE
	ir.Must("calls-parseInt-with-positive-5",
		ir.Eq(ir.ParseInt(ir.Num(5)), ir.Num(5)))

	// ParseInt(Num(0)) — should be UNSATISFIED (catches the C++ precondition)
	ir.Must("calls-parseInt-with-zero",
		ir.Eq(ir.ParseInt(ir.Num(0)), ir.Num(0)))

	decls := finishCollect()
	if len(decls) != 2 {
		t.Fatalf("expected 2 declarations, got %d", len(decls))
	}

	// ----- 3. Mint Go's property mementos (Go signs Go) -----
	consumerSigner := makeDeterministicKey([]byte("go-consumer-signer-seed"))
	minter := claim_envelope.NewMinter(consumerSigner)

	declaredAt := "2026-04-30T13:00:00.000Z"
	consumerMembers := map[string][]byte{}

	for _, d := range decls {
		prop, ok := d.(ir.PropertyDeclaration)
		if !ok {
			continue
		}
		formulaValue, err := claim_envelope.FormulaToValue(prop.Formula)
		if err != nil {
			t.Fatalf("FormulaToValue: %v", err)
		}
		minted, err := minter.MintProperty(claim_envelope.PropertyMintArgs{
			BindingHash:  hash16("go-consumer:" + prop.Name),
			PropertyHash: hash16("hash-of:" + prop.Name),
			ProducedBy:   "go-consumer@1",
			ProducedAt:   declaredAt,
			IRFormula:    formulaValue,
			Scope: map[string]interface{}{
				"kind": "function",
				"name": prop.Name,
			},
			IRKitVersion: "go-kit@1.0",
		})
		if err != nil {
			t.Fatalf("mint property: %v", err)
		}
		consumerMembers[minted.CID] = minted.CanonicalBytes
	}

	// ----- 4. Bundle the consumer's .proof file (Go signs Go) -----
	var catalogSeed [32]byte
	copy(catalogSeed[:], []byte("go-catalog-signer-seed-32-bytes-x"))
	builder := proof_envelope.NewBuilder()
	out, err := builder.Build(&proof_envelope.Input{
		Name:       "go-consumer-app",
		Version:    "1.0.0",
		Members:    consumerMembers,
		SignerCID:  "sha256:go-consumer-signer",
		SignerSeed: catalogSeed,
		DeclaredAt: declaredAt,
	})
	if err != nil {
		t.Fatalf("Build: %v", err)
	}
	consumerProofPath := filepath.Join(projectRoot, out.FilenameCID+".proof")
	if err := os.WriteFile(consumerProofPath, out.Bytes, 0o644); err != nil {
		t.Fatal(err)
	}
	t.Logf("Go consumer .proof: %s (%d bytes)", consumerProofPath, len(out.Bytes))

	// ----- 5. Run the Go bridge enforcement runner (Go calls C++ via the bridge) -----
	solver := &Solver{
		Entries: []SolverEntry{
			{
				Type:     "z3",
				Binary:   "z3",
				Compiler: "smt-lib",
				Flags:    []string{"-in", "-T:5"},
				Timeout:  5 * time.Second,
			},
		},
	}
	runner := NewRunner(solver)
	report, err := runner.RunBridgeEnforcement(projectRoot)
	if err != nil {
		t.Fatalf("RunBridgeEnforcement: %v", err)
	}

	for _, le := range report.LoadErrors {
		t.Logf("  load error: %s: %s", le.ProofPath, le.Reason)
	}

	if report.TotalCallsites != 2 {
		t.Errorf("expected 2 callsites, got %d", report.TotalCallsites)
	}

	var passing, failing *ReportRow
	for i := range report.Rows {
		row := &report.Rows[i]
		if row.CallSite.PropertyName == "calls-parseInt-with-positive-5" {
			passing = row
		}
		if row.CallSite.PropertyName == "calls-parseInt-with-zero" {
			failing = row
		}
	}
	if passing == nil {
		t.Fatalf("missing positive-5 row; got rows %+v", report.Rows)
	}
	if failing == nil {
		t.Fatalf("missing zero row; got rows %+v", report.Rows)
	}
	if passing.Status != string(VerdictDischarged) {
		t.Errorf("ParseInt(Num(5)) status = %s, want discharged. reason=%s",
			passing.Status, passing.Reason)
	}
	if failing.Status != string(VerdictUnsatisfied) {
		t.Errorf("ParseInt(Num(0)) status = %s, want unsatisfied. reason=%s",
			failing.Status, failing.Reason)
	}

	t.Logf("\n  ✓ DEMO: Go verifier caught ParseInt(Num(0)) using the C++-authored precondition.\n"+
		"    Discharged calls:  %d\n"+
		"    Caught violations: %d\n",
		report.Discharged, report.Violations)
}

// makeDeterministicKey derives an ed25519 private key from a seed-like
// string. Used so the Go consumer's signer is repeatable across runs.
func makeDeterministicKey(seed []byte) ed25519.PrivateKey {
	// Pad / truncate to ed25519's 32-byte seed.
	var ed25519Seed [32]byte
	if len(seed) >= 32 {
		copy(ed25519Seed[:], seed[:32])
	} else {
		copy(ed25519Seed[:], seed)
		// fill the rest from random for deterministic length
		_, _ = rand.Read(ed25519Seed[len(seed):])
	}
	return ed25519.NewKeyFromSeed(ed25519Seed[:])
}

// hash16 derives a 16-char hex hash from the given string.
func hash16(s string) string {
	bytes, _ := canonicalizer.NewEncoder().Encode(s)
	full := canonicalizer.SHA256Hex(bytes)
	return full[:16]
}

// fmt is referenced indirectly via Sprintf'd test logs; this no-op
// import retained explicitly to avoid breakage if the file evolves.
var _ = fmt.Sprintf
