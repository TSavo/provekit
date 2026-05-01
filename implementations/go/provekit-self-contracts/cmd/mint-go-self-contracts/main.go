// mint-go-self-contracts — the Go peer self-contracts orchestrator.
//
// Mirrors implementations/rust/provekit-self-contracts/src/bin/
// mint-self-contracts.rs.
//
// 1. Walks every Invariants_<label>() function in the slabs package.
// 2. Authors all contracts; mints them as signed mementos under the
//    foundation key (test seed [0x42; 32]).
// 3. Bundles every contract memento into a single `.proof` whose
//    filename IS the catalog CID. No bridges (Go has no public-API
//    counterpart of Rust's `parse_formula` closed-loop).
// 4. Asserts byte-determinism by minting twice into separate temp dirs
//    and comparing the resulting catalog CIDs. Fails loud on mismatch.
//
// Cross-language conformance: the .proof bytes are produced by the
// existing Go canonicalizer / claim_envelope / proof_envelope. Any
// conformant verifier (Rust / C++) consumes them; the catalog CID is
// the protocol-mandated content-address.
//
// Run:
//   go run ./cmd/mint-go-self-contracts
//   go run ./cmd/mint-go-self-contracts /tmp/provekit-go-self
package main

import (
	"crypto/ed25519"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/provekit/go-self-contracts/slabs"
	"github.com/provekit/ir-symbolic/canonicalizer"
	"github.com/provekit/ir-symbolic/claim_envelope"
	"github.com/provekit/ir-symbolic/ir"
	"github.com/provekit/ir-symbolic/proof_envelope"
)

const (
	producedBy = "provekit-go-self-contracts@1.0"
	declaredAt = "2026-04-30T18:00:00.000Z"
	catalogName    = "@provekit/go-self-contracts"
	catalogVersion = "1.0.0"
)

// foundationSeed mirrors the Rust orchestrator's `signer_seed = [0x42; 32]`
// for cross-language reproducibility.
func foundationSeed() [32]byte {
	var seed [32]byte
	for i := range seed {
		seed[i] = 0x42
	}
	return seed
}

// authoredSlab is one source-file's drained collector + label.
type authoredSlab struct {
	label    string
	path     string
	contracts []ir.ContractDeclaration
}

// authorAllInvariants drives every Invariants_<label>() function in the
// slabs package. Each slab is collected in isolation: ResetCollector +
// BeginCollecting + Run + drain. The quantifier counter resets inside
// BeginCollecting, so successive runs produce byte-identical IR.
func authorAllInvariants() ([]authoredSlab, error) {
	out := make([]authoredSlab, 0, len(slabs.Slabs()))
	for _, s := range slabs.Slabs() {
		ir.ResetCollector()
		finish := ir.BeginCollecting()
		s.Run()
		decls := finish()
		// Coerce — every slab authors only contracts (no bridges).
		contracts := make([]ir.ContractDeclaration, 0, len(decls))
		for _, d := range decls {
			c, ok := d.(ir.ContractDeclaration)
			if !ok {
				return nil, fmt.Errorf(
					"slab %q: unexpected declaration kind %s (want contract)",
					s.Label, d.Kind())
			}
			contracts = append(contracts, c)
		}
		out = append(out, authoredSlab{
			label: s.Label, path: s.Path, contracts: contracts,
		})
	}
	return out, nil
}

// mintResult is the outcome of one mint-self-proof run.
type mintResult struct {
	cid          string
	bytesLen     int
	path         string
	memberCount  int
	contractCIDs map[string]string
	perSourceCounts []labeledCount
	totalContracts  int
}

type labeledCount struct {
	label string
	count int
}

// mintSelfProof mints every authored contract as a signed memento,
// bundles the lot into a `.proof` whose filename is the catalog CID,
// writes it to outDir, and returns the result.
func mintSelfProof(outDir string) (*mintResult, error) {
	if err := os.MkdirAll(outDir, 0o755); err != nil {
		return nil, fmt.Errorf("mkdir %s: %w", outDir, err)
	}

	authored, err := authorAllInvariants()
	if err != nil {
		return nil, err
	}

	seed := foundationSeed()
	signer := ed25519.NewKeyFromSeed(seed[:])
	minter := claim_envelope.NewMinter(signer)

	members := map[string][]byte{}
	contractCIDs := map[string]string{}
	perSource := make([]labeledCount, 0, len(authored))
	totalContracts := 0

	for _, slab := range authored {
		perSource = append(perSource, labeledCount{label: slab.label, count: len(slab.contracts)})
		totalContracts += len(slab.contracts)

		for _, c := range slab.contracts {
			// Convert kit IR formulas to JSON-shape values via the
			// existing FormulaToValue helper (round-trips json.Marshal /
			// json.Unmarshal). Empty slots stay nil; the minter rejects
			// "all nil" loud.
			var preV, postV, invV interface{}
			if c.Pre != nil {
				preV, err = claim_envelope.FormulaToValue(c.Pre)
				if err != nil {
					return nil, fmt.Errorf("FormulaToValue Pre %s: %w", c.Name, err)
				}
			}
			if c.Post != nil {
				postV, err = claim_envelope.FormulaToValue(c.Post)
				if err != nil {
					return nil, fmt.Errorf("FormulaToValue Post %s: %w", c.Name, err)
				}
			}
			if c.Inv != nil {
				invV, err = claim_envelope.FormulaToValue(c.Inv)
				if err != nil {
					return nil, fmt.Errorf("FormulaToValue Inv %s: %w", c.Name, err)
				}
			}

			minted, err := minter.MintContract(claim_envelope.ContractMintArgs{
				ContractName: c.Name,
				Pre:          preV,
				Post:         postV,
				Inv:          invV,
				OutBinding:   c.OutBinding,
				ProducedBy:   producedBy,
				ProducedAt:   declaredAt,
				AuthoringKind: claim_envelope.AuthoringKitAuthor,
				AuthoringKitAuthor: claim_envelope.AuthoringKitAuthorArgs{
					Author: producedBy,
					Note:   fmt.Sprintf("self-contract from %s", slab.path),
				},
			})
			if err != nil {
				return nil, fmt.Errorf("MintContract %s: %w", c.Name, err)
			}

			// Detect duplicate names ACROSS slabs and fail loud (mirrors Rust).
			if _, dup := contractCIDs[c.Name]; dup {
				return nil, fmt.Errorf(
					"duplicate contract name %q across slabs", c.Name)
			}
			contractCIDs[c.Name] = minted.CID
			members[minted.CID] = minted.CanonicalBytes
		}
	}

	// SignerCID is a content-address of the foundation pubkey.
	pub := signer.Public().(ed25519.PublicKey)
	signerCID := canonicalizer.ComputeCID(pub)

	builder := proof_envelope.NewBuilder()
	out, err := builder.Build(&proof_envelope.Input{
		Name:       catalogName,
		Version:    catalogVersion,
		Members:    members,
		SignerCID:  signerCID,
		SignerSeed: seed,
		DeclaredAt: declaredAt,
	})
	if err != nil {
		return nil, fmt.Errorf("Build: %w", err)
	}

	if !strings.HasPrefix(out.FilenameCID, "blake3-512:") {
		return nil, fmt.Errorf("internal: filename CID missing blake3-512 prefix: %s", out.FilenameCID)
	}

	path := filepath.Join(outDir, out.FilenameCID+".proof")
	if err := os.WriteFile(path, out.Bytes, 0o644); err != nil {
		return nil, fmt.Errorf("write %s: %w", path, err)
	}

	return &mintResult{
		cid:             out.FilenameCID,
		bytesLen:        len(out.Bytes),
		path:            path,
		memberCount:     len(members),
		contractCIDs:    contractCIDs,
		perSourceCounts: perSource,
		totalContracts:  totalContracts,
	}, nil
}

func main() {
	argv := os.Args
	var outDir string
	if len(argv) >= 2 {
		outDir = argv[1]
	} else {
		// Default: implementations/go/target/ relative to cwd, walking
		// up to find the worktree root marker (Cargo.toml with [workspace]).
		cwd, err := os.Getwd()
		if err != nil {
			fmt.Fprintf(os.Stderr, "ERROR: getwd: %v\n", err)
			os.Exit(1)
		}
		root := cwd
		for {
			if _, err := os.Stat(filepath.Join(root, "Cargo.toml")); err == nil {
				if data, err := os.ReadFile(filepath.Join(root, "Cargo.toml")); err == nil {
					if strings.Contains(string(data), "[workspace]") {
						break
					}
				}
			}
			parent := filepath.Dir(root)
			if parent == root {
				root = "/tmp/provekit-go-self-proofs"
				break
			}
			root = parent
		}
		outDir = filepath.Join(root, "implementations", "go", "target")
	}

	fmt.Println("== ProvekIt Go peer self-contracts orchestrator ==")
	fmt.Println()
	fmt.Printf("output dir: %s\n", outDir)

	// Author + report counts before mint.
	authored, err := authorAllInvariants()
	if err != nil {
		fmt.Fprintf(os.Stderr, "ERROR: author: %v\n", err)
		os.Exit(1)
	}
	fmt.Println()
	fmt.Println("authored:")
	total := 0
	for _, s := range authored {
		total += len(s.contracts)
		fmt.Printf("  %22s  %2d contracts  (%s)\n", s.label, len(s.contracts), s.path)
	}
	fmt.Printf("  %22s  %2d contracts (TOTAL)\n", "[ALL]", total)

	// Determinism check: mint twice into distinct dirs.
	detDir := filepath.Join(os.TempDir(), fmt.Sprintf("provekit-go-self-determinism-%d", os.Getpid()))
	_ = os.RemoveAll(detDir)

	mintA, err := mintSelfProof(detDir)
	if err != nil {
		fmt.Fprintf(os.Stderr, "ERROR: mint determinism A: %v\n", err)
		_ = os.RemoveAll(detDir)
		os.Exit(1)
	}
	mintB, err := mintSelfProof(outDir)
	if err != nil {
		fmt.Fprintf(os.Stderr, "ERROR: mint: %v\n", err)
		_ = os.RemoveAll(detDir)
		os.Exit(1)
	}

	fmt.Println()
	fmt.Println("minted:")
	fmt.Printf("  .proof file:        %s\n", mintB.path)
	fmt.Printf("  bytes:              %d\n", mintB.bytesLen)
	fmt.Printf("  members:            %d\n", mintB.memberCount)
	fmt.Printf("  total contracts:    %d\n", mintB.totalContracts)
	fmt.Printf("  catalog CID:        %s\n", mintB.cid)

	if mintA.cid != mintB.cid {
		fmt.Fprintln(os.Stderr)
		fmt.Fprintf(os.Stderr, "ERROR: byte-determinism check FAILED:\n")
		fmt.Fprintf(os.Stderr, "  run A CID: %s\n", mintA.cid)
		fmt.Fprintf(os.Stderr, "  run B CID: %s\n", mintB.cid)
		_ = os.RemoveAll(detDir)
		os.Exit(2)
	}
	_ = os.RemoveAll(detDir)
	fmt.Printf("  determinism check:  OK (two runs produced identical CIDs)\n")

	// Stable enumeration of contracts by CID for the report.
	cids := make([]string, 0, len(mintB.contractCIDs))
	for c := range mintB.contractCIDs {
		cids = append(cids, c)
	}
	sort.Strings(cids)

	fmt.Println()
	fmt.Println("== done. Go self-application: live. ==")
}
