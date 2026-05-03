// mint-go-self-contracts — the Go peer self-contracts orchestrator.
//
// Mirrors implementations/rust/provekit-self-contracts/src/bin/
// mint-self-contracts.rs.
//
//  1. Walks every Invariants_<label>() function in the slabs package.
//  2. Authors all contracts AND bridges; mints them as signed mementos
//     under the foundation key (test seed [0x42; 32]). Bridges may have
//     a `pending-go-counterpart:<name>` placeholder in TargetContractCid
//     that gets resolved to a real memento CID after every counterpart
//     contract has been minted (mirrors rust lib.rs:368-385 closed-loop
//     bridge resolution).
//  3. Bundles every memento into a single `.proof` whose filename IS
//     the catalog CID. Bridges include the phase-2 cross-kit bridges
//     declared by slabs/lift_plugin_protocol.go.
//  4. Asserts byte-determinism by minting twice into separate temp dirs
//     and comparing the resulting catalog CIDs. Fails loud on mismatch.
//
// Cross-language conformance: the .proof bytes are produced by the
// existing Go canonicalizer / claim_envelope / proof_envelope. Any
// conformant verifier (Rust / C++) consumes them; the catalog CID is
// the protocol-mandated content-address.
//
// Run:
//
//	go run ./cmd/mint-go-self-contracts
//	go run ./cmd/mint-go-self-contracts /tmp/provekit-go-self
package main

import (
	"crypto/ed25519"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/tsavo/provekit/go/provekit-ir-symbolic/canonicalizer"
	"github.com/tsavo/provekit/go/provekit-ir-symbolic/claim_envelope"
	"github.com/tsavo/provekit/go/provekit-ir-symbolic/ir"
	"github.com/tsavo/provekit/go/provekit-ir-symbolic/proof_envelope"
	"github.com/tsavo/provekit/go/provekit-self-contracts/slabs"
)

const (
	producedBy     = "provekit-go-self-contracts@1.0"
	declaredAt     = "2026-04-30T18:00:00.000Z"
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
//
// A slab may carry contract declarations, bridge declarations, or both.
// Phase-2 cross-kit bridges (slabs/lift_plugin_protocol.go) declare
// only bridges; existing per-source-file slabs declare only contracts.
// The orchestrator handles each kind separately at mint time.
type authoredSlab struct {
	label     string
	path      string
	contracts []ir.ContractDeclaration
	bridges   []ir.BridgeDeclaration
}

// authorAllInvariants drives every Invariants_<label>() function in the
// slabs package. Each slab is collected in isolation: ResetCollector +
// BeginCollecting + Run + drain. The quantifier counter resets inside
// BeginCollecting, so successive runs produce byte-identical IR.
//
// Each declaration is sorted into its kind's bucket. Contract and bridge
// are the v1.1.0 declaration kinds the kit currently emits; any other
// kind (or a future addition) errors loud here so the orchestrator
// stays the source of truth for what shapes get minted.
func authorAllInvariants() ([]authoredSlab, error) {
	out := make([]authoredSlab, 0, len(slabs.Slabs()))
	for _, s := range slabs.Slabs() {
		ir.ResetCollector()
		finish := ir.BeginCollecting()
		s.Run()
		decls := finish()
		var (
			contracts []ir.ContractDeclaration
			bridges   []ir.BridgeDeclaration
		)
		for _, d := range decls {
			switch typed := d.(type) {
			case ir.ContractDeclaration:
				contracts = append(contracts, typed)
			case ir.BridgeDeclaration:
				bridges = append(bridges, typed)
			default:
				return nil, fmt.Errorf(
					"slab %q: unsupported declaration kind %s (want contract or bridge)",
					s.Label, d.Kind())
			}
		}
		out = append(out, authoredSlab{
			label:     s.Label,
			path:      s.Path,
			contracts: contracts,
			bridges:   bridges,
		})
	}
	return out, nil
}

// mintResult is the outcome of one mint-self-proof run.
type mintResult struct {
	cid             string
	bytesLen        int
	path            string
	memberCount     int
	contractCIDs    map[string]string
	bridgeCIDs      map[string]string // bridgeName -> memento CID
	perSourceCounts []labeledCount
	totalContracts  int
	totalBridges    int
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
	bridgeCIDs := map[string]string{}
	perSource := make([]labeledCount, 0, len(authored))
	totalContracts := 0
	totalBridges := 0

	// PASS 1: mint every contract.
	//
	// Bridges are deferred to PASS 2 because phase-2 cross-kit bridges
	// reference go counterpart contracts via a `pending-go-counterpart:`
	// placeholder in TargetContractCid; the placeholder can only be
	// rewritten once the target counterpart contract has a real memento
	// CID. Slab order ensures all contract slabs run before any bridge
	// slab in PASS 1 here, but we explicitly skip bridge declarations
	// in this pass for clarity (and so a future slab that mixes both
	// declaration kinds in the same Invariants_*() function still works).
	for _, slab := range authored {
		perSource = append(perSource, labeledCount{label: slab.label, count: len(slab.contracts) + len(slab.bridges)})
		totalContracts += len(slab.contracts)
		totalBridges += len(slab.bridges)

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
				ContractName:  c.Name,
				Pre:           preV,
				Post:          postV,
				Inv:           invV,
				OutBinding:    c.OutBinding,
				ProducedBy:    producedBy,
				ProducedAt:    declaredAt,
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

	// PASS 2: mint every bridge.
	//
	// Phase-2 cross-kit bridges may carry a `pending-go-counterpart:<name>`
	// placeholder in TargetContractCid; resolve it here to the real
	// memento CID of the named go counterpart contract minted in PASS 1.
	// Bridges authored without the placeholder (e.g. plain rust-style
	// closed-loop bridges) pass through untouched.
	for _, slab := range authored {
		for _, b := range slab.bridges {
			targetCid := b.TargetContractCid
			if strings.HasPrefix(targetCid, slabs.PendingTargetContractCidPrefix) {
				counterpartName := strings.TrimPrefix(targetCid, slabs.PendingTargetContractCidPrefix)
				resolved, ok := contractCIDs[counterpartName]
				if !ok {
					return nil, fmt.Errorf(
						"bridge %q: cannot resolve target counterpart %q (no contract minted with that name)",
						b.Name, counterpartName)
				}
				targetCid = resolved
			}
			minted, err := minter.MintBridge(claim_envelope.BridgeMintArgs{
				ProducedBy:        producedBy,
				ProducedAt:        declaredAt,
				SourceSymbol:      b.SourceSymbol,
				SourceLayer:       b.SourceLayer,
				SourceContractCID: b.SourceContractCid,
				TargetContractCID: targetCid,
				TargetProofCID:    b.TargetProofCid,
				TargetLayer:       b.TargetLayer,
				IRArgSorts:        []interface{}{},
				IRReturnSort:      "Bool",
				Notes:             b.Notes,
			})
			if err != nil {
				return nil, fmt.Errorf("MintBridge %s: %w", b.Name, err)
			}
			if _, dup := bridgeCIDs[b.Name]; dup {
				return nil, fmt.Errorf(
					"duplicate bridge name %q across slabs", b.Name)
			}
			bridgeCIDs[b.Name] = minted.CID
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
		bridgeCIDs:      bridgeCIDs,
		perSourceCounts: perSource,
		totalContracts:  totalContracts,
		totalBridges:    totalBridges,
	}, nil
}

func main() {
	argv := os.Args

	// --rpc takes over stdin/stdout for the lift-plugin protocol.
	// Spec: protocol/specs/2026-04-30-lift-plugin-protocol.md
	for _, a := range argv {
		if a == "--rpc" {
			runRPCMode()
			return
		}
	}

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
	totalContracts := 0
	totalBridges := 0
	for _, s := range authored {
		totalContracts += len(s.contracts)
		totalBridges += len(s.bridges)
		fmt.Printf("  %32s  %2d contracts  %2d bridges  (%s)\n",
			s.label, len(s.contracts), len(s.bridges), s.path)
	}
	fmt.Printf("  %32s  %2d contracts  %2d bridges  (TOTAL)\n",
		"[ALL]", totalContracts, totalBridges)

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
	fmt.Printf("  total bridges:      %d\n", mintB.totalBridges)
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
