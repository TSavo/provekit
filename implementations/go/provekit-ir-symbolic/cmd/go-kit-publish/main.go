// go-kit-publish authors the parseInt precondition in pure Go and
// publishes it as a v1.1.0 .proof file that any conformant verifier
// can consume. The reverse of the C++ kit: Go is the author, every
// other language is a downstream consumer.
//
// Output: a .proof bundle containing one contract memento (parseInt's
// pre formula `forall n: Int. n > 0`) plus one bridge memento
// (TS-layer parseInt → that contract).
//
// Usage: go run ./cmd/go-kit-publish <out-dir>
package main

import (
	"crypto/ed25519"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/tsavo/provekit/go/provekit-ir-symbolic/claim_envelope"
	"github.com/tsavo/provekit/go/provekit-ir-symbolic/ir"
	"github.com/tsavo/provekit/go/provekit-ir-symbolic/proof_envelope"
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

	ir.Must("parseInt",
		ir.ForAll(ir.Int, func(n ir.IrTerm) ir.IrFormula {
			return ir.Gt(n, ir.Num(0))
		}))

	decls := finishCollect()
	if len(decls) != 1 {
		fmt.Fprintf(os.Stderr, "expected 1 declaration, got %d\n", len(decls))
		os.Exit(1)
	}

	// ---- Mint contract + bridge in pure Go (v1.1.0) ----
	var seed [32]byte
	copy(seed[:], []byte("go-kit-author-seed-32-bytes-pad!"))
	signer := ed25519.NewKeyFromSeed(seed[:])
	minter := claim_envelope.NewMinter(signer)

	declaredAt := "2026-04-30T14:00:00.000Z"
	producedBy := "go-kit@1.0"

	contract := decls[0].(ir.ContractDeclaration)

	preValue, err := claim_envelope.FormulaToValue(contract.Pre)
	if err != nil {
		fmt.Fprintf(os.Stderr, "FormulaToValue: %v\n", err)
		os.Exit(1)
	}

	mintedContract, err := minter.MintContract(claim_envelope.ContractMintArgs{
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
		fmt.Fprintf(os.Stderr, "MintContract: %v\n", err)
		os.Exit(1)
	}
	fmt.Printf("  contract minted: %s -> CID %s\n", contract.Name, mintedContract.CID)

	mintedBridge, err := minter.MintBridge(claim_envelope.BridgeMintArgs{
		ProducedBy:        producedBy,
		ProducedAt:        declaredAt,
		SourceSymbol:      "parseInt",
		SourceLayer:       "ts",
		TargetContractCID: mintedContract.CID,
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
			mintedContract.CID: mintedContract.CanonicalBytes,
			mintedBridge.CID:   mintedBridge.CanonicalBytes,
		},
		// SignerCID is a content-address of a signer-pubkey memento.
		// v1.1.0 placeholder: real key-binding plumbing lands later;
		// for now a sentinel under the v1.1.0 hash tag.
		SignerCID:  "blake3-512:" + strings.Repeat("0", 127) + "1",
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
