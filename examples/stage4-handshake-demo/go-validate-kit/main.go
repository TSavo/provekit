// Stage 4 demo: validate-kit publisher (Go).
//
// Publishes a contract memento for `validateInput`, with a `post`
// formula instead of a `pre` (the existing go-kit-publish example
// only authors a pre slot, so we duplicate the small bit of glue we
// need here without mutating the canonical example).
//
// The post formula varies by --shape:
//
//   gt0   (Run A and Run D-fixed): forall n: Int. n > 0
//   gte1  (Run B):                  forall n: Int. n >= 1
//   gte0  (Run C):                  forall n: Int. n >= 0
//
// Plus a bridge memento mapping the IR ctor `validateInput` ->
// the contract memento. The output is a v1.1.0 .proof file in the
// directory passed as --out.
//
// Usage:
//   go run . --shape gt0 --out /tmp/run-A
//
// All bytes here are language-neutral protocol bytes: a Rust verifier
// loads the .proof, walks the catalog, and uses the bridge to find
// the post formula for handshake purposes.
package main

import (
	"crypto/ed25519"
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/sugar/ir-symbolic/claim_envelope"
	"github.com/sugar/ir-symbolic/ir"
	"github.com/sugar/ir-symbolic/proof_envelope"
)

func mustExit(err error, msg string) {
	if err != nil {
		fmt.Fprintf(os.Stderr, "%s: %v\n", msg, err)
		os.Exit(1)
	}
}

func main() {
	shape := flag.String("shape", "gt0", "post-formula shape: gt0 | gte1 | gte0")
	out := flag.String("out", ".", "output directory for the .proof")
	flag.Parse()

	mustExit(os.MkdirAll(*out, 0o755), "mkdir out")

	// ---- Author the post formula via kit primitives ----
	ir.ResetCollector()
	finishCollect := ir.BeginCollecting()

	var post ir.IrFormula
	switch *shape {
	case "gt0":
		post = ir.ForAll(ir.Int, func(n ir.IrTerm) ir.IrFormula {
			return ir.Gt(n, ir.Num(0))
		})
	case "gte1":
		post = ir.ForAll(ir.Int, func(n ir.IrTerm) ir.IrFormula {
			return ir.Gte(n, ir.Num(1))
		})
	case "gte0":
		post = ir.ForAll(ir.Int, func(n ir.IrTerm) ir.IrFormula {
			return ir.Gte(n, ir.Num(0))
		})
	default:
		fmt.Fprintf(os.Stderr, "unknown --shape %q (want gt0|gte1|gte0)\n", *shape)
		os.Exit(1)
	}

	// We don't need the collector's machinery for a single contract,
	// but we keep the call balanced.
	_ = finishCollect

	postValue, err := claim_envelope.FormulaToValue(post)
	mustExit(err, "FormulaToValue(post)")

	// ---- Mint the contract (post-only) ----
	var seed [32]byte
	copy(seed[:], []byte("go-validate-kit-author-seed-32!!"))
	signer := ed25519.NewKeyFromSeed(seed[:])
	minter := claim_envelope.NewMinter(signer)
	_ = signer // signer used by minter; suppress unused-var warnings if any

	declaredAt := "2026-04-30T14:00:00.000Z"
	producedBy := "go-validate-kit@1.0"

	mintedContract, err := minter.MintContract(claim_envelope.ContractMintArgs{
		ContractName:  "validateInput",
		Post:          postValue,
		OutBinding:    "out",
		ProducedBy:    producedBy,
		ProducedAt:    declaredAt,
		AuthoringKind: claim_envelope.AuthoringKitAuthor,
		AuthoringKitAuthor: claim_envelope.AuthoringKitAuthorArgs{
			Author: producedBy,
		},
	})
	mustExit(err, "MintContract")
	fmt.Printf("  contract minted: validateInput -> CID %s\n", mintedContract.CID)

	// ---- Mint the bridge for the IR ctor "validateInput" ----
	mintedBridge, err := minter.MintBridge(claim_envelope.BridgeMintArgs{
		ProducedBy:        producedBy,
		ProducedAt:        declaredAt,
		SourceSymbol:      "validateInput",
		SourceLayer:       "ts",
		TargetContractCID: mintedContract.CID,
		TargetLayer:       "go-kit",
		IRArgSorts:        []interface{}{"String"},
		IRReturnSort:      "String",
	})
	mustExit(err, "MintBridge")
	fmt.Printf("  bridge   minted: validateInput -> CID %s\n", mintedBridge.CID)

	// ---- Bundle into a .proof ----
	signerCidStr := "blake3-512:" + strings.Repeat("0", 127) + "5"

	builder := proof_envelope.NewBuilder()
	bundle, err := builder.Build(&proof_envelope.Input{
		Name:    "@example/go-validate-kit",
		Version: "1.0.0",
		Members: map[string][]byte{
			mintedContract.CID: mintedContract.CanonicalBytes,
			mintedBridge.CID:   mintedBridge.CanonicalBytes,
		},
		SignerCID:  signerCidStr,
		SignerSeed: seed,
		DeclaredAt: declaredAt,
	})
	mustExit(err, "build .proof")

	outPath := filepath.Join(*out, bundle.FilenameCID+".proof")
	mustExit(os.WriteFile(outPath, bundle.Bytes, 0o644), "write .proof")
	fmt.Printf("\n  wrote .proof: %s (%d bytes, cid=%s)\n",
		outPath, len(bundle.Bytes), bundle.FilenameCID)
}
