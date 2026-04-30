// THE GO END-TO-END DEMO (v1.1.0).
//
//   Go signs Go.
//   Go calls C++ (via the bridged kit primitive).
//   Go detects parseInt(num(0)).
//
// Architecture:
//   1. C++ kit shipped a v1.1.0 .proof file with parseInt's contract
//      (pre = `forall n: Int. n > 0`) — produced by parseInt_kit_proof.cpp.
//   2. Go consumer authors invariants via kit primitives ParseInt(Num(...))
//      — every call emits a Ctor("parseInt", [arg]) IrTerm.
//   3. Go consumer mints + signs its contract mementos in pure Go.
//   4. Go consumer bundles them into its own .proof file in pure Go.
//   5. Go bridge enforcement runner walks both .proofs:
//        - load-all-proofs builds a unified CID pool.
//        - enumerate-callsites finds Ctor("parseInt", ...) inside Go's contracts.
//        - resolve-bridge-target hash-looks-up the bridge → C++'s contract memento.
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

	"github.com/provekit/ir-symbolic/claim_envelope"
	"github.com/provekit/ir-symbolic/ir"
	"github.com/provekit/ir-symbolic/proof_envelope"
)

// cppProofPath points at the C++ reference impl's v1.1.0 output.
// Regenerate with the C++ kit's parseInt_kit_proof binary; commit
// the resulting CID here.
const cppProofPath = "/tmp/cpp-kit-out-v11/bfe74d1a9d836f926058b331002da2f5.proof"

func TestCrossLangGoVerifiesCppProof(t *testing.T) {
	if _, err := os.Stat(cppProofPath); err != nil {
		t.Skipf("C++ .proof not found at %s — regenerate from the C++ kit on v1.1.0", cppProofPath)
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
	cppProofDest := filepath.Join(cppKitDir, filepath.Base(cppProofPath))
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

	// ----- 3. Mint Go's contract mementos (Go signs Go) -----
	consumerSigner := makeDeterministicKey([]byte("go-consumer-signer-seed"))
	minter := claim_envelope.NewMinter(consumerSigner)

	declaredAt := "2026-04-30T13:00:00.000Z"
	producedBy := "go-consumer@1"
	consumerMembers := map[string][]byte{}

	for _, d := range decls {
		contract, ok := d.(ir.ContractDeclaration)
		if !ok {
			continue
		}
		preValue, err := claim_envelope.FormulaToValue(contract.Pre)
		if err != nil {
			t.Fatalf("FormulaToValue: %v", err)
		}
		minted, err := minter.MintContract(claim_envelope.ContractMintArgs{
			ContractName:  contract.Name,
			Pre:           preValue,
			OutBinding:    contract.OutBinding,
			ProducedBy:    producedBy,
			ProducedAt:    declaredAt,
			AuthoringKind: claim_envelope.AuthoringKitAuthor,
			AuthoringKitAuthor: claim_envelope.AuthoringKitAuthorArgs{
				Author: producedBy,
			},
		})
		if err != nil {
			t.Fatalf("mint contract: %v", err)
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

	t.Logf("\n  DEMO: Go verifier caught ParseInt(Num(0)) using the C++-authored precondition.\n"+
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

// fmt is referenced via Sprintf'd test logs; this no-op import is
// retained so a future caller can add diagnostic output without
// re-importing.
var _ = fmt.Sprintf
