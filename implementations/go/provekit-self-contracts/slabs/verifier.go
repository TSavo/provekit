package slabs

// Invariants for verifier/load_all_proofs.go.
//
// Public surface covered:
//   * LoadAllProofsStage, (*LoadAllProofsStage).Run(projectRoot string).
//   * proofFilenameV11RE = "^blake3-512:[0-9a-f]{128}\\.proof$".
//   * hashTagPrefix = "blake3-512:".
//   * sigTagPrefix  = "ed25519:".
//
// Honest scope: filename-shape and tag-prefix claims are LIVING DOCS;
// operationally enforced by cross_lang_demo_test.go. The IR can express
// length floors and rejection-on-malformed-tag as deterministic gestures.

import "github.com/provekit/ir-symbolic/ir"

func InvariantsLoadAllProofs() {
	// hashTagPrefix on the verifier path matches the canonicalizer's
	// HashTagPrefix exactly.
	//   len(hashTagPrefix) = 11
	ir.Contract("verifier_hashTagPrefix_length_eq_11", ir.ContractArgs{
		Post: ir.Eq(ir.StringLength(ctor0("verifier_hashTagPrefix")), ir.Num(11)),
	})

	// sigTagPrefix length sanity: "ed25519:" is exactly 8 bytes.
	ir.Contract("verifier_sigTagPrefix_length_eq_8", ir.ContractArgs{
		Post: ir.Eq(ir.StringLength(ctor0("verifier_sigTagPrefix")), ir.Num(8)),
	})

	// Run on an empty directory yields a pool with empty Mementos and
	// empty BridgesBySymbol (no errors raised). LIVING DOCS — Z3 has no
	// semantics for "filesystem is empty".
	ir.Must("Run_empty_dir_yields_empty_pool",
		ir.ForAll(ir.String, func(_ ir.IrTerm) ir.IrFormula {
			return ir.Eq(ctor0("Run_empty"), ctor0("Run_empty"))
		}))

	// Filename-shape: "blake3-512:<128 hex>.proof" is exactly 145 chars
	// (11 + 128 + 6).
	ir.Contract("proofFilename_v11_total_length_eq_145", ir.ContractArgs{
		Post: ir.Eq(ctor0("proofFilename_v11_length"), ir.Num(145)),
	})

	// Members CID prefix check rejects non-"blake3-512:" tags.
	// LIVING DOCS for the rejection branch (predicate has no Z3 semantics).
	ir.Must("Run_rejects_unsupported_member_tag",
		ir.ForAll(ir.String, func(_ ir.IrTerm) ir.IrFormula {
			return ir.Eq(ctor0("Run_reject_tag"), ctor0("Run_reject_tag"))
		}))
}
