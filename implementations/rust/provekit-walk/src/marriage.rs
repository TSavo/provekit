// SPDX-License-Identifier: Apache-2.0
//
// AST + MIR marriage. The point of running both layers isn't to
// produce two parallel contracts that happen to agree — that's
// coexistence. Marriage produces ONE FunctionContractMemento per
// function, with the predicate atoms from BOTH layers unioned and
// content-addressed as a single substrate record.
//
// What gets married
//   - Surface-AST lift via syn (lift.rs + capture-avoiding subst)
//   - Post-borrow-check MIR lift via Charon LLBC (llbc_lift.rs)
//
// The merged contract carries the AST's locus (it has source-line
// metadata; LLBC has spans but they're block-level, not function-
// level), formals, formal sorts, return sort, and body_cid. The pre
// and post formulas are the deduplicated union of conjuncts from
// both layers — atoms that BOTH layers see appear once; atoms only
// MIR sees (overflow checks, deref soundness) are added; atoms only
// AST sees (source-level patterns MIR optimized away) are kept.
//
// Per paper 07 §6, the substrate stores one memento per logical
// contract. Marriage is what makes that operational: downstream
// consumers see ONE content_cid per function, not two.
//
// Why this matters
//   - Cross-layer cache reuse on agreed predicates: minted once.
//   - MIR-exclusive predicates layer cleanly into the same memento
//     (overflow asserts, slice-bounds checks, drop-glue invariants
//     — all things rustc inserts that the surface AST doesn't see).
//   - The substrate is one body of evidence, not two.

use std::collections::HashSet;
use std::path::Path;

use provekit_ir_types::IrFormula;
use thiserror::Error;

use crate::canonical::{cid_of_value, formula_to_canonical, jcs_bytes_of_value};
use crate::charon_runner::{invoke_charon_on_rs_source, RunnerError};
use crate::contract::{build_function_contract_with_file, FunctionContractMemento};
use crate::llbc::LlbcError;
use crate::llbc_lift::lift_llbc_function;
use crate::wp::atomic_true;

#[derive(Debug, Error)]
pub enum MarriageError {
    #[error("source read: {0}")]
    Io(#[from] std::io::Error),
    #[error("syn parse: {0}")]
    Syn(#[from] syn::Error),
    #[error("function not found in source: {0}")]
    FunctionNotFound(String),
    #[error("charon runner: {0}")]
    Runner(#[from] RunnerError),
    #[error("llbc: {0}")]
    Llbc(#[from] LlbcError),
}

/// How AST and LLBC layers compared on the conjuncts of pre and post.
/// `Identical` means byte-equal pre AND post; `LlbcExtra` means LLBC
/// contributed atoms AST didn't see (the common case for arithmetic
/// or slice-indexing code where MIR sees overflow / bounds checks);
/// `AstExtra` is rare but possible if MIR optimized something away;
/// `Both` is a divergence requiring investigation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayerAgreement {
    Identical,
    LlbcExtra,
    AstExtra,
    Both,
}

/// The married result. Both diagnostic layer contracts and the merged
/// substrate-facing contract are exposed; downstream consumers should
/// use `merged`, while review tooling and cache instrumentation can
/// inspect `ast` and `llbc` independently.
pub struct MarriedContract {
    pub ast: FunctionContractMemento,
    pub llbc: FunctionContractMemento,
    pub merged: FunctionContractMemento,
    pub agreement: LayerAgreement,
}

/// Lift one function from a `.rs` source through both layers and
/// produce a married contract. Charon must be available; see
/// `charon_runner::find_charon_binary`.
pub fn lift_marriage(
    rs_source_path: &Path,
    fn_name: &str,
) -> Result<MarriedContract, MarriageError> {
    // AST layer
    let src = std::fs::read_to_string(rs_source_path)?;
    let file: syn::File = syn::parse_str(&src)?;
    let item_fn = file
        .items
        .into_iter()
        .find_map(|item| match item {
            syn::Item::Fn(f) if f.sig.ident == fn_name => Some(f),
            _ => None,
        })
        .ok_or_else(|| MarriageError::FunctionNotFound(fn_name.to_string()))?;
    let ast = build_function_contract_with_file(&item_fn, None, rs_source_path.to_str());

    // LLBC layer via Charon
    let krate = invoke_charon_on_rs_source(rs_source_path, None)?;
    let f = krate
        .function_by_name(fn_name)
        .map_err(MarriageError::Llbc)?;
    let llbc = lift_llbc_function(f, rs_source_path.to_str()).map_err(MarriageError::Llbc)?;

    Ok(marry(ast, llbc))
}

/// Marry two already-built contracts. Useful when the layers come from
/// elsewhere (e.g. a vendored `.llbc` for tests, or a different lifter
/// stack). The merged contract takes the AST's metadata (locus,
/// formals, sorts, body_cid) and the union of both layers' predicate
/// atoms.
pub fn marry(
    ast: FunctionContractMemento,
    llbc: FunctionContractMemento,
) -> MarriedContract {
    let agreement = compare_layers(&ast, &llbc);
    let merged_pre = merge_formulas(&ast.pre, &llbc.pre);
    let merged_post = merge_formulas(&ast.post, &llbc.post);

    let mut merged = ast.clone();
    merged.pre = merged_pre;
    merged.post = merged_post;
    let value = crate::contract::build_memento_value(&merged);
    merged.canonical_bytes = jcs_bytes_of_value(&value);
    merged.cid = cid_of_value(&value);

    MarriedContract {
        ast,
        llbc,
        merged,
        agreement,
    }
}

fn compare_layers(
    ast: &FunctionContractMemento,
    llbc: &FunctionContractMemento,
) -> LayerAgreement {
    let ast_pre_atoms = atom_byte_set(&ast.pre);
    let llbc_pre_atoms = atom_byte_set(&llbc.pre);
    let ast_post_atoms = atom_byte_set(&ast.post);
    let llbc_post_atoms = atom_byte_set(&llbc.post);

    let ast_only = !ast_pre_atoms.difference(&llbc_pre_atoms).next().is_none()
        || !ast_post_atoms.difference(&llbc_post_atoms).next().is_none();
    let llbc_only = !llbc_pre_atoms.difference(&ast_pre_atoms).next().is_none()
        || !llbc_post_atoms.difference(&ast_post_atoms).next().is_none();

    match (ast_only, llbc_only) {
        (false, false) => LayerAgreement::Identical,
        (false, true) => LayerAgreement::LlbcExtra,
        (true, false) => LayerAgreement::AstExtra,
        (true, true) => LayerAgreement::Both,
    }
}

fn atom_byte_set(f: &IrFormula) -> HashSet<Vec<u8>> {
    conjuncts(f)
        .into_iter()
        .filter(|c| !is_trivially_true(c))
        .map(|c| jcs_bytes_of_value(&formula_to_canonical(&c)))
        .collect()
}

/// Flatten an IrFormula into its top-level conjuncts. Non-And nodes
/// are returned as a singleton vec.
fn conjuncts(f: &IrFormula) -> Vec<IrFormula> {
    match f {
        IrFormula::And { operands } => operands.iter().flat_map(conjuncts).collect(),
        other => vec![other.clone()],
    }
}

fn is_trivially_true(f: &IrFormula) -> bool {
    matches!(f, IrFormula::Atomic { name, args } if name == "true" && args.is_empty())
}

/// Conjunction-merge two formulas: union of conjuncts deduplicated by
/// JCS-byte hash, with `true` atoms dropped (identity for ∧). Source
/// order is preserved (AST atoms first, then LLBC's extras).
pub fn merge_formulas(a: &IrFormula, b: &IrFormula) -> IrFormula {
    let mut atoms: Vec<IrFormula> = Vec::new();
    let mut seen: HashSet<Vec<u8>> = HashSet::new();
    for f in conjuncts(a).into_iter().chain(conjuncts(b)) {
        if is_trivially_true(&f) {
            continue;
        }
        let bytes = jcs_bytes_of_value(&formula_to_canonical(&f));
        if seen.insert(bytes) {
            atoms.push(f);
        }
    }
    if atoms.is_empty() {
        atomic_true().into_formula()
    } else if atoms.len() == 1 {
        atoms.into_iter().next().unwrap()
    } else {
        IrFormula::And { operands: atoms }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical::{formula_to_canonical, jcs_bytes_of_value};
    use crate::charon_runner::find_charon_binary;
    use crate::contract::build_function_contract_with_file;
    use crate::envelope::{
        wrap_function_contract_cached, EnvelopeCache, DEV_SIGNER_SEED,
    };
    use crate::llbc::LlbcCrate;
    use crate::llbc_lift::lift_llbc_function;

    fn fixture_path(name: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(name)
    }

    fn build_layers(rs: &str, llbc: &str, fn_name: &str) -> (FunctionContractMemento, FunctionContractMemento) {
        let src = std::fs::read_to_string(fixture_path(rs)).unwrap();
        let file: syn::File = syn::parse_str(&src).unwrap();
        let item_fn = file
            .items
            .into_iter()
            .find_map(|item| match item {
                syn::Item::Fn(f) if f.sig.ident == fn_name => Some(f),
                _ => None,
            })
            .unwrap();
        let ast = build_function_contract_with_file(&item_fn, None, Some(rs));
        let krate = LlbcCrate::from_path(fixture_path(llbc)).unwrap();
        let f = krate.function_by_name(fn_name).unwrap();
        let llbc = lift_llbc_function(f, Some(rs)).unwrap();
        (ast, llbc)
    }

    #[test]
    fn marriage_on_agreed_layers_yields_identical() {
        let (ast, llbc) = build_layers("clean.rs", "clean.llbc", "f");
        let married = marry(ast, llbc);
        assert_eq!(
            married.agreement,
            LayerAgreement::Identical,
            "AST and LLBC agree on `if x<10 panic` => Identical"
        );
    }

    #[test]
    fn marriage_on_compound_or_yields_identical() {
        let (ast, llbc) = build_layers("compound_or.rs", "compound_or.llbc", "h");
        let married = marry(ast, llbc);
        assert_eq!(married.agreement, LayerAgreement::Identical);
    }

    #[test]
    fn merged_contract_carries_one_cid_for_substrate() {
        // The OPERATIONAL claim. The merged contract is what
        // downstream consumers see. ONE memento, ONE cid, ONE
        // substrate record per function — regardless of how many
        // layers contributed.
        let (ast, llbc) = build_layers("clean.rs", "clean.llbc", "f");
        let married = marry(ast, llbc);

        // Merged.cid is well-formed and stable.
        assert!(married.merged.cid.starts_with("blake3-512:"));
        assert_eq!(married.merged.fn_name, "f");

        // When layers agree, merged predicate bytes equal each layer's.
        let merged_pre = jcs_bytes_of_value(&formula_to_canonical(&married.merged.pre));
        let ast_pre = jcs_bytes_of_value(&formula_to_canonical(&married.ast.pre));
        let llbc_pre = jcs_bytes_of_value(&formula_to_canonical(&married.llbc.pre));
        assert_eq!(merged_pre, ast_pre);
        assert_eq!(merged_pre, llbc_pre);
    }

    #[test]
    fn married_contract_wraps_to_one_envelope_via_cache() {
        // Substrate-level claim: the married contract wraps once,
        // and wrapping it twice hits the cache. One mint, one record.
        let (ast, llbc) = build_layers("clean.rs", "clean.llbc", "f");
        let married = marry(ast, llbc);

        let mut cache = EnvelopeCache::new();
        let env1 = wrap_function_contract_cached(
            &married.merged,
            "2026-05-05T00:00:00Z",
            &DEV_SIGNER_SEED,
            &mut cache,
        )
        .unwrap();
        assert_eq!(cache.mints, 1);
        assert_eq!(cache.hits, 0);

        let env2 = wrap_function_contract_cached(
            &married.merged,
            "2026-05-05T00:00:00Z",
            &DEV_SIGNER_SEED,
            &mut cache,
        )
        .unwrap();
        assert_eq!(cache.mints, 1, "second wrap of merged must hit cache");
        assert_eq!(cache.hits, 1);
        assert_eq!(env1.cid, env2.cid);
        assert_eq!(env1.contract_cid, env2.contract_cid);
    }

    #[test]
    fn merge_formulas_dedups_identical_atoms() {
        // Direct unit on the conjunct merge: same atom from two
        // layers becomes one.
        use crate::wp::{atomic_ge, const_int, var};
        let a = atomic_ge(var("x"), const_int(10)).into_formula();
        let b = atomic_ge(var("x"), const_int(10)).into_formula();
        let merged = merge_formulas(&a, &b);
        // Single-atom conjunction collapses back to the bare atom.
        match merged {
            IrFormula::Atomic { name, .. } => assert_eq!(name, "≥"),
            other => panic!("expected single Atomic, got {:?}", other),
        }
    }

    #[test]
    fn merge_formulas_unions_distinct_atoms() {
        // x ≥ 10 (from AST) ∧ y ≥ 5 (from MIR-only) merges into the
        // union. This is the structural case of LlbcExtra: MIR sees
        // a predicate AST didn't.
        use crate::wp::{atomic_ge, const_int, var};
        let a = atomic_ge(var("x"), const_int(10)).into_formula();
        let b = atomic_ge(var("y"), const_int(5)).into_formula();
        let merged = merge_formulas(&a, &b);
        match merged {
            IrFormula::And { operands } => {
                assert_eq!(operands.len(), 2);
            }
            other => panic!("expected And, got {:?}", other),
        }
    }

    #[test]
    fn marriage_on_overflow_arithmetic_surfaces_mir_extras_in_merged() {
        // The "MIR sees more" lane. `fn g(x: u32) -> u32 { x * 2 }`
        // has no source-level preconditions — AST walk emits trivial-
        // true pre. MIR inserts an overflow assert (CheckedMul +
        // Assert on the overflow flag), so LLBC contributes a no-
        // overflow predicate AST never saw.
        //
        // Agreement here is `Both`, not `LlbcExtra`, because the AST
        // walk also derives `result = x * 2` from the trailing
        // return expression — and the LLBC lift doesn't yet emit
        // return-value derivations from MIR's tuple-projection +
        // CheckedOp pattern (tracked separately under #383). On the
        // pre side LLBC has more; on the post side AST has more.
        //
        // The OPERATIONAL claim — what the substrate sees — is that
        // the merged contract carries BOTH layers' contributions.
        // No information is lost in marriage.
        let (ast, llbc) = build_layers("overflow.rs", "overflow.llbc", "g");
        let married = marry(ast, llbc);

        // After the LLBC return-value derivation lands (#383), the
        // post side stops contributing AST-only atoms (both layers
        // see `result = x*2`); the only asymmetry is MIR's no-overflow
        // atom on the pre side. Agreement collapses to LlbcExtra.
        assert_eq!(
            married.agreement,
            LayerAgreement::LlbcExtra,
            "post sides converge after return-value derivation; MIR-only is the no-overflow"
        );

        // The MIR-only no-overflow atom is in the merged contract.
        // This is the empirical "MIR sees more" demonstration —
        // observable in the merged record, not asserted.
        let merged_pre_str = serde_json::to_string(&married.merged.pre).unwrap();
        assert!(
            merged_pre_str.contains("no-overflow:mul-wrap"),
            "merged pre carries the MIR-only no-overflow atom: {}",
            merged_pre_str
        );

        // AST's `result = x*2` derivation also survives in merged.
        // The marriage is symmetric: it doesn't drop AST's atoms in
        // favor of LLBC's, or vice versa.
        let merged_post_str = serde_json::to_string(&married.merged.post).unwrap();
        assert!(
            merged_post_str.contains("result"),
            "merged post still carries AST's result-equation: {}",
            merged_post_str
        );
    }

    #[test]
    fn marriage_on_slice_index_surfaces_bounds_check_atom() {
        // `fn at(s: &[u32], i: usize) -> u32 { s[i] }` — AST sees no
        // source-level guard. MIR inserts a BoundsCheck assert
        // (i < s.len()). Marriage should surface that as a MIR-only
        // atom in the merged contract's pre.
        let (ast, llbc) = build_layers("bounds.rs", "bounds.llbc", "at");
        let married = marry(ast, llbc);

        let merged_pre_str = serde_json::to_string(&married.merged.pre).unwrap();
        // The bounds atom: `Atomic("<", [Var("i"), Ctor("len", [Var("s")])])`.
        assert!(
            merged_pre_str.contains("\"i\""),
            "merged pre references the index formal: {}",
            merged_pre_str
        );
        assert!(
            merged_pre_str.contains("len"),
            "merged pre includes the slice length via PtrMetadata lift: {}",
            merged_pre_str
        );
        // LLBC contributes at least one atom AST didn't.
        assert!(
            matches!(
                married.agreement,
                LayerAgreement::LlbcExtra | LayerAgreement::Both
            ),
            "MIR sees the bounds check; AST doesn't. got {:?}",
            married.agreement
        );
    }

    #[test]
    fn marriage_on_division_surfaces_div_by_zero_atom() {
        // `fn d(x: u32, y: u32) -> u32 { x / y }` — AST sees no guard.
        // MIR inserts DivisionByZero assert. Marriage merges in the
        // `y ≠ 0` predicate.
        let (ast, llbc) = build_layers("divmod.rs", "divmod.llbc", "d");
        let married = marry(ast, llbc);

        let merged_pre_str = serde_json::to_string(&married.merged.pre).unwrap();
        // The div-by-zero atom: `Atomic("≠", [Var("y"), Const(0)])`.
        assert!(
            merged_pre_str.contains("\"y\""),
            "merged pre references the divisor formal: {}",
            merged_pre_str
        );
        assert!(
            merged_pre_str.contains("\"\\u2260\"") || merged_pre_str.contains("≠"),
            "merged pre includes the ≠ predicate: {}",
            merged_pre_str
        );
        assert!(
            matches!(
                married.agreement,
                LayerAgreement::LlbcExtra | LayerAgreement::Both
            ),
            "MIR sees the div-by-zero check; AST doesn't. got {:?}",
            married.agreement
        );
    }

    #[test]
    fn end_to_end_marriage_via_charon_runner() {
        // Full pipeline: write .rs → AST lift + Charon → LLBC lift →
        // marry → ONE married contract. No vendored fixture.
        if find_charon_binary().is_none() {
            eprintln!("charon not found; skipping marriage e2e");
            return;
        }
        let path = fixture_path("clean.rs");
        let married = lift_marriage(&path, "f").expect("marriage runs end-to-end");
        assert_eq!(married.agreement, LayerAgreement::Identical);
        assert!(married.merged.cid.starts_with("blake3-512:"));
    }
}
