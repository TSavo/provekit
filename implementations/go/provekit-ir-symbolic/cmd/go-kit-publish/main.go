// go-kit-publish authors the parseInt precondition in pure Go and
// publishes it as a .proof file that any conformant verifier can
// consume. The reverse of the C++ kit: Go is the author, every other
// language is a downstream consumer.
//
// Usage: go run ./cmd/go-kit-publish <out-dir>
package main

import (
	"crypto/ed25519"
	"fmt"
	"os"
	"path/filepath"

	"github.com/provekit/ir-symbolic/canonicalizer"
	"github.com/provekit/ir-symbolic/claim_envelope"
	"github.com/provekit/ir-symbolic/ir"
	"github.com/provekit/ir-symbolic/proof_envelope"
)

func main() {
	if len(os.Args) < 2 {
		fmt.Fprintln(os.Stderr, "Usage: go-kit-publish <out-dir>")
		os.Exit(1)
	}
	outDir := os.Args[1]
	if err := os.MkdirAll(outDir, 0o755); err != nil {
		fmt.Fprintf(os.Stderr, "mkdir %s: %v\n", outDir, err)
		os.Exit(1)
	}

	// ---- Author the precondition via kit primitives ----
	ir.ResetCollector()
	finishCollect := ir.BeginCollecting()

	ir.Must("parseInt-requires-positive",
		ir.ForAll(ir.Int, func(n ir.IrTerm) ir.IrFormula {
			return ir.Gt(n, ir.Num(0))
		}))

	decls := finishCollect()
	if len(decls) != 1 {
		fmt.Fprintf(os.Stderr, "expected 1 declaration, got %d\n", len(decls))
		os.Exit(1)
	}

	// ---- Mint property + bridge in pure Go ----
	var seed [32]byte
	copy(seed[:], []byte("go-kit-author-seed-32-bytes-pad!"))
	signer := ed25519.NewKeyFromSeed(seed[:])
	minter := claim_envelope.NewMinter(signer)

	declaredAt := "2026-04-30T14:00:00.000Z"
	prop := decls[0].(ir.PropertyDeclaration)

	formulaValue, err := claim_envelope.FormulaToValue(prop.Formula)
	if err != nil {
		fmt.Fprintf(os.Stderr, "FormulaToValue: %v\n", err)
		os.Exit(1)
	}

	mintedProperty, err := minter.MintProperty(claim_envelope.PropertyMintArgs{
		BindingHash:  hash16("go-kit-property:" + prop.Name),
		PropertyHash: hash16("hash-of:" + prop.Name),
		ProducedBy:   "go-kit@1.0",
		ProducedAt:   declaredAt,
		IRFormula:    formulaValue,
		Scope: map[string]interface{}{
			"kind": "function",
			"name": prop.Name,
		},
		IRKitVersion: "go-kit@1.0",
	})
	if err != nil {
		fmt.Fprintf(os.Stderr, "MintProperty: %v\n", err)
		os.Exit(1)
	}
	fmt.Printf("  property minted: %s -> CID %s\n", prop.Name, mintedProperty.CID)

	mintedBridge, err := minter.MintBridge(claim_envelope.BridgeMintArgs{
		BindingHash:       hash16("ts:parseInt"),
		PropertyHash:      hash16("bridge:parseInt"),
		ProducedBy:        "go-kit@1.0",
		ProducedAt:        declaredAt,
		SourceSymbol:      "parseInt",
		SourceLayer:       "ts",
		TargetContractCID: mintedProperty.CID,
		TargetLayer:       "go-kit",
		IRArgSorts:        []interface{}{"String"},
		IRReturnSort:      "Int",
	})
	if err != nil {
		fmt.Fprintf(os.Stderr, "MintBridge: %v\n", err)
		os.Exit(1)
	}
	fmt.Printf("  bridge   minted: parseInt -> CID %s\n", mintedBridge.CID)

	// ---- Bundle into a .proof file ----
	var catalogSeed [32]byte
	copy(catalogSeed[:], []byte("go-kit-catalog-seed-32-bytes-pa!"))
	builder := proof_envelope.NewBuilder()
	out, err := builder.Build(&proof_envelope.Input{
		Name:    "@example/go-kit",
		Version: "1.0.0",
		Members: map[string][]byte{
			mintedProperty.CID: mintedProperty.CanonicalBytes,
			mintedBridge.CID:   mintedBridge.CanonicalBytes,
		},
		SignerCID:  "sha256:go-kit-signer",
		SignerSeed: catalogSeed,
		DeclaredAt: declaredAt,
	})
	if err != nil {
		fmt.Fprintf(os.Stderr, "Build: %v\n", err)
		os.Exit(1)
	}

	outPath := filepath.Join(outDir, out.FilenameCID+".proof")
	if err := os.WriteFile(outPath, out.Bytes, 0o644); err != nil {
		fmt.Fprintf(os.Stderr, "write %s: %v\n", outPath, err)
		os.Exit(1)
	}
	fmt.Printf("\n  wrote .proof: %s (%d bytes, cid=%s)\n",
		outPath, len(out.Bytes), out.FilenameCID)
}

func hash16(s string) string {
	bytes, _ := canonicalizer.NewEncoder().Encode(s)
	return canonicalizer.SHA256Hex(bytes)[:16]
}
