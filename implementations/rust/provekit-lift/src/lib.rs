// SPDX-License-Identifier: Apache-2.0
//
// provekit-lift: workspace lift toolchain.
//
// Walks a Rust workspace (or single crate), parses every `.rs` file
// with syn, dispatches each parsed file to all registered adapters,
// collects ContractDecls, mints them via provekit_claim_envelope, and
// bundles the result into a single signed `.proof` catalog.
//
// STRATEGIC POSITIONING (read this before extending):
//
//   ProvekIt does NOT compete with proptest, contracts, kani, prusti,
//   hypothesis-py, deal-py, bean-validation-java, zod-ts, etc. It sits
//   BENEATH them. Developers keep their existing annotation library;
//   `provekit-lift` reads what's already there and promotes it to a
//   content-addressed signed contract.
//
//   The macros in `provekit-macros` are a fallback for greenfield code
//   where the developer doesn't already have an annotation library.
//   Adoption path looks like: lift first, mint-via-macros only when
//   greenfield.
//
// LIBRARY API:
//
//   `lift_path(workspace_root, options)` walks the directory tree,
//   runs every adapter, and returns a `LiftReport`.
//
//   `mint_proof(decls, options)` takes lifted decls and mints a signed
//   `.proof` byte blob plus its CID.
//
//   `lift_and_mint(workspace_root, options)` is the convenience that
//   does both and writes the file.
//
// CARGO SUBCOMMAND:
//
//   `cargo provekit-lift [--workspace <dir>] [--target-dir <out>]`
//   wires through the bin `cargo-provekit-lift`. We also ship a plain
//   `provekit-lift` bin for direct invocation.

use base64::Engine;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use provekit_canonicalizer::blake3_512_of;
use provekit_claim_envelope::{
    compute_contract_set_cid, contract_cid as compute_contract_cid, mint_contract, Authoring,
    MintContractArgs,
};
use provekit_ir_symbolic::{serialize::formula_to_value, ContractDecl, Formula};
use std::rc::Rc;
use provekit_proof_envelope::{
    build_proof_envelope, ed25519_pubkey_string, Ed25519Seed, ProofEnvelopeInput,
};

pub mod call_edges;
pub use call_edges::{
    extract_call_edges_from_file, mint_call_edge, CallEdgeMemento, CallSiteLocus,
};

pub use provekit_lift_contracts as adapter_contracts;
pub use provekit_lift_creusot as adapter_creusot;
pub use provekit_lift_flux as adapter_flux;
pub use provekit_lift_kani as adapter_kani;
pub use provekit_lift_proptest as adapter_proptest;
pub use provekit_lift_prusti as adapter_prusti;
pub use provekit_lift_quickcheck as adapter_quickcheck;
pub use provekit_lift_rust_tests as adapter_rust_tests;
pub use provekit_lift_verus as adapter_verus;

/// Per-adapter outcome. Counts what each adapter saw and what was
/// liftable. The `warnings` are the honest "I saw it but couldn't
/// translate" log surfaced to the caller.
#[derive(Debug, Default, Clone)]
pub struct AdapterReport {
    pub adapter: &'static str,
    pub seen: usize,
    pub lifted: usize,
    pub warnings: Vec<AdapterWarning>,
}

#[derive(Debug, Clone)]
pub struct AdapterWarning {
    pub adapter: &'static str,
    pub source_path: String,
    pub item_name: String,
    pub reason: String,
}

#[derive(Debug, Default)]
pub struct LiftReport {
    pub decls: Vec<ContractDecl>,
    /// Call-edge mementos extracted per spec #114 §1 R1.
    /// One memento per call site within a contracted function.
    pub call_edges: Vec<CallEdgeMemento>,
    pub adapter_reports: Vec<AdapterReport>,
    pub files_scanned: usize,
    pub parse_errors: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct LiftOptions {
    /// Producer identity included in each minted memento.
    pub produced_by: String,
    /// ISO-8601 with millisecond precision and trailing 'Z'.
    pub produced_at: String,
    /// Ed25519 signing seed. Default is the shared dev seed
    /// `[0x42; 32]` so demos are reproducible across machines.
    pub signer_seed: Ed25519Seed,
    /// Lifter identity recorded in `Authoring::Lift`.
    pub lifter: String,
    /// Catalog name written into the `.proof` envelope.
    pub catalog_name: String,
    /// Catalog version string.
    pub catalog_version: String,
}

impl Default for LiftOptions {
    fn default() -> Self {
        Self {
            produced_by: "provekit-lift@0.1.0".into(),
            produced_at: "2026-04-30T00:00:00.000Z".into(),
            signer_seed: [0x42; 32],
            lifter: "provekit-lift".into(),
            catalog_name: "@provekit/lift".into(),
            catalog_version: "0.1.0".into(),
        }
    }
}

/// Walk the workspace at `root`, running every adapter on every `.rs`
/// file. Skips `target/`, `.git/`, and the toolchain's own crates so
/// fixtures planted in this crate don't pollute caller workspaces.
pub fn lift_path(root: &Path) -> LiftReport {
    let mut report = LiftReport::default();

    // Per-adapter accumulators keyed by adapter name.
    let mut proptest_seen = 0usize;
    let mut proptest_lifted = 0usize;
    let mut proptest_warnings: Vec<AdapterWarning> = Vec::new();
    let mut contracts_seen = 0usize;
    let mut contracts_lifted = 0usize;
    let mut contracts_warnings: Vec<AdapterWarning> = Vec::new();
    let mut kani_seen = 0usize;
    let mut kani_lifted = 0usize;
    let mut kani_warnings: Vec<AdapterWarning> = Vec::new();
    let mut prusti_seen = 0usize;
    let mut prusti_lifted = 0usize;
    let mut prusti_warnings: Vec<AdapterWarning> = Vec::new();
    let mut creusot_seen = 0usize;
    let mut creusot_lifted = 0usize;
    let mut creusot_warnings: Vec<AdapterWarning> = Vec::new();
    let mut flux_seen = 0usize;
    let mut flux_lifted = 0usize;
    let mut flux_warnings: Vec<AdapterWarning> = Vec::new();
    let mut quickcheck_seen = 0usize;
    let mut quickcheck_lifted = 0usize;
    let mut quickcheck_warnings: Vec<AdapterWarning> = Vec::new();
    let mut verus_seen = 0usize;
    let mut verus_lifted = 0usize;
    let mut verus_warnings: Vec<AdapterWarning> = Vec::new();
    let mut rust_tests_seen = 0usize;
    let mut rust_tests_lifted = 0usize;
    let mut rust_tests_warnings: Vec<AdapterWarning> = Vec::new();
    let mut rust_tests_l2_seen = 0usize;
    let mut rust_tests_l2_lifted = 0usize;
    let mut rust_tests_l2_warnings: Vec<AdapterWarning> = Vec::new();
    // Pattern-split counters for the CLI summary (printed as a single
    // breakdown line; not currently surfaced in AdapterReport).
    let mut l2_bounded_loop_lifted = 0usize;
    let mut l2_bounded_loop_skipped = 0usize;
    let mut l2_helper_lifted = 0usize;
    let mut l2_helper_skipped = 0usize;
    let mut l2_char_lifted = 0usize;
    let mut l2_char_skipped = 0usize;

    // Retain (path_str, parsed_file) so the second pass (call-edge extraction)
    // can re-use them without re-parsing. Only successfully parsed files are kept.
    let mut parsed_files: Vec<(String, syn::File)> = Vec::new();

    for (rel_posix, abs_path) in enumerate_rs_files(root) {
        report.files_scanned += 1;
        let bytes = match std::fs::read(&abs_path) {
            Ok(b) => b,
            Err(e) => {
                report
                    .parse_errors
                    .push((rel_posix.clone(), format!("read: {e}")));
                continue;
            }
        };
        let src = match std::str::from_utf8(&bytes) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let file = match syn::parse_file(src) {
            Ok(f) => f,
            Err(e) => {
                report
                    .parse_errors
                    .push((rel_posix.clone(), format!("parse: {e}")));
                continue;
            }
        };
        // Use the relative POSIX path (not the absolute host path) as the
        // path_str passed to every adapter and to call-edge extraction.
        // This is the fix for cross-platform CID non-determinism: absolute
        // paths embed the machine's directory prefix; relative POSIX paths
        // are identical for identical source trees on any host.
        let path_str = rel_posix.clone();
        parsed_files.push((path_str.clone(), file.clone()));

        // Adapter: proptest.
        let p_out = adapter_proptest::lift_file(&file, &path_str);
        proptest_seen += p_out.seen;
        proptest_lifted += p_out.lifted;
        for w in p_out.warnings {
            proptest_warnings.push(AdapterWarning {
                adapter: "proptest",
                source_path: w.source_path,
                item_name: w.item_name,
                reason: w.reason,
            });
        }
        report.decls.extend(p_out.decls);

        // Adapter: contracts.
        let c_out = adapter_contracts::lift_file(&file, &path_str);
        contracts_seen += c_out.seen;
        contracts_lifted += c_out.lifted;
        for w in c_out.warnings {
            contracts_warnings.push(AdapterWarning {
                adapter: "contracts",
                source_path: w.source_path,
                item_name: w.item_name,
                reason: w.reason,
            });
        }
        report.decls.extend(c_out.decls);

        // Adapter: kani.
        let k_out = adapter_kani::lift_file(&file, &path_str);
        kani_seen += k_out.seen;
        kani_lifted += k_out.lifted;
        for w in k_out.warnings {
            kani_warnings.push(AdapterWarning {
                adapter: "kani",
                source_path: w.source_path,
                item_name: w.item_name,
                reason: w.reason,
            });
        }
        report.decls.extend(k_out.decls);

        // Adapter: prusti.
        let pr_out = adapter_prusti::lift_file(&file, &path_str);
        prusti_seen += pr_out.seen;
        prusti_lifted += pr_out.lifted;
        for w in pr_out.warnings {
            prusti_warnings.push(AdapterWarning {
                adapter: "prusti",
                source_path: w.source_path,
                item_name: w.item_name,
                reason: w.reason,
            });
        }
        report.decls.extend(pr_out.decls);

        // Adapter: creusot.
        let cr_out = adapter_creusot::lift_file(&file, &path_str);
        creusot_seen += cr_out.seen;
        creusot_lifted += cr_out.lifted;
        for w in cr_out.warnings {
            creusot_warnings.push(AdapterWarning {
                adapter: "creusot",
                source_path: w.source_path,
                item_name: w.item_name,
                reason: w.reason,
            });
        }
        report.decls.extend(cr_out.decls);

        // Adapter: flux.
        let fx_out = adapter_flux::lift_file(&file, &path_str);
        flux_seen += fx_out.seen;
        flux_lifted += fx_out.lifted;
        for w in fx_out.warnings {
            flux_warnings.push(AdapterWarning {
                adapter: "flux",
                source_path: w.source_path,
                item_name: w.item_name,
                reason: w.reason,
            });
        }
        report.decls.extend(fx_out.decls);

        // Adapter: quickcheck.
        let qc_out = adapter_quickcheck::lift_file(&file, &path_str);
        quickcheck_seen += qc_out.seen;
        quickcheck_lifted += qc_out.lifted;
        for w in qc_out.warnings {
            quickcheck_warnings.push(AdapterWarning {
                adapter: "quickcheck",
                source_path: w.source_path,
                item_name: w.item_name,
                reason: w.reason,
            });
        }
        report.decls.extend(qc_out.decls);

        // Adapter: verus.
        let vr_out = adapter_verus::lift_file(&file, &path_str);
        verus_seen += vr_out.seen;
        verus_lifted += vr_out.lifted;
        for w in vr_out.warnings {
            verus_warnings.push(AdapterWarning {
                adapter: "verus",
                source_path: w.source_path,
                item_name: w.item_name,
                reason: w.reason,
            });
        }
        report.decls.extend(vr_out.decls);

        // Adapter: rust-tests / Layer 2 (bounded loops, helper inlining,
        // characterization conjunction). Run BEFORE Layer 0 so it can
        // claim test fns it owns; Layer 0 then skips the claimed names.
        let l2_out = adapter_rust_tests::lift_file_layer2(&file, &path_str);
        rust_tests_l2_seen += l2_out.seen;
        rust_tests_l2_lifted += l2_out.lifted;
        l2_bounded_loop_lifted += l2_out.bounded_loop_lifted;
        l2_bounded_loop_skipped += l2_out.bounded_loop_skipped;
        l2_helper_lifted += l2_out.helper_inlined_lifted;
        l2_helper_skipped += l2_out.helper_inlined_skipped;
        l2_char_lifted += l2_out.characterization_lifted;
        l2_char_skipped += l2_out.characterization_skipped;
        for w in l2_out.warnings {
            rust_tests_l2_warnings.push(AdapterWarning {
                adapter: "rust-tests-layer2",
                source_path: w.source_path,
                item_name: w.item_name,
                reason: w.reason,
            });
        }
        report.decls.extend(l2_out.decls);
        let claimed = l2_out.claimed_tests;

        // Adapter: rust-tests / Layer 0 (#[test] / #[tokio::test] -> per-assertion
        // mementos). Skip tests that Layer 2 claimed.
        let rt_out = adapter_rust_tests::lift_file_with_skip(&file, &path_str, &claimed);
        rust_tests_seen += rt_out.seen;
        rust_tests_lifted += rt_out.lifted;
        for w in rt_out.warnings {
            rust_tests_warnings.push(AdapterWarning {
                adapter: "rust-tests",
                source_path: w.source_path,
                item_name: w.item_name,
                reason: w.reason,
            });
        }
        report.decls.extend(rt_out.decls);
    }

    report.adapter_reports.push(AdapterReport {
        adapter: "proptest",
        seen: proptest_seen,
        lifted: proptest_lifted,
        warnings: proptest_warnings,
    });
    report.adapter_reports.push(AdapterReport {
        adapter: "contracts",
        seen: contracts_seen,
        lifted: contracts_lifted,
        warnings: contracts_warnings,
    });
    report.adapter_reports.push(AdapterReport {
        adapter: "kani",
        seen: kani_seen,
        lifted: kani_lifted,
        warnings: kani_warnings,
    });
    report.adapter_reports.push(AdapterReport {
        adapter: "prusti",
        seen: prusti_seen,
        lifted: prusti_lifted,
        warnings: prusti_warnings,
    });
    report.adapter_reports.push(AdapterReport {
        adapter: "creusot",
        seen: creusot_seen,
        lifted: creusot_lifted,
        warnings: creusot_warnings,
    });
    report.adapter_reports.push(AdapterReport {
        adapter: "flux",
        seen: flux_seen,
        lifted: flux_lifted,
        warnings: flux_warnings,
    });
    report.adapter_reports.push(AdapterReport {
        adapter: "quickcheck",
        seen: quickcheck_seen,
        lifted: quickcheck_lifted,
        warnings: quickcheck_warnings,
    });
    report.adapter_reports.push(AdapterReport {
        adapter: "verus",
        seen: verus_seen,
        lifted: verus_lifted,
        warnings: verus_warnings,
    });
    report.adapter_reports.push(AdapterReport {
        adapter: "rust-tests",
        seen: rust_tests_seen,
        lifted: rust_tests_lifted,
        warnings: rust_tests_warnings,
    });
    report.adapter_reports.push(AdapterReport {
        adapter: "rust-tests-layer2",
        seen: rust_tests_l2_seen,
        lifted: rust_tests_l2_lifted,
        warnings: rust_tests_l2_warnings,
    });
    // Cargo-cult unused-suppress: pattern-split counters are surfaced
    // through the CLI summary path (run_cli below), not here. Keep them
    // alive so dead-code lint stays clean.
    let _ = (
        l2_bounded_loop_lifted,
        l2_bounded_loop_skipped,
        l2_helper_lifted,
        l2_helper_skipped,
        l2_char_lifted,
        l2_char_skipped,
    );

    // --- Second pass: call-edge extraction (spec #114 §1 R1) ----------------
    //
    // Build a signer-independent name → contractCid map from all lifted decls.
    // This covers the full compilation unit; cross-crate callees won't appear
    // here and will produce targetContractCid: null edges.
    let contract_cid_map: BTreeMap<String, String> = {
        let mut map = BTreeMap::new();
        for d in &report.decls {
            // Compute the signer-independent content CID using a minimal
            // MintContractArgs (only the content-bearing fields matter;
            // signer_seed, produced_at, etc. do not affect the CID per spec #94).
            let args = MintContractArgs {
                formals: Vec::new(),
                emit_empty_formals: false,
                formal_sorts: Vec::new(),
                contract_name: d.name.clone(),
                pre: d.pre.as_deref().map(formula_to_value),
                post: d.post.as_deref().map(formula_to_value),
                inv: d.inv.as_deref().map(formula_to_value),
                out_binding: d.out_binding.clone(),
                produced_by: "provekit-lift".into(),
                produced_at: "2026-01-01T00:00:00.000Z".into(),
                input_cids: vec![],
                authoring: Authoring::Lift {
                    lifter: "provekit-lift".into(),
                    evidence: String::new(),
                    source_cid: None,
                },
                signer_seed: [0u8; 32],
            };
            let ccid = compute_contract_cid(&args);
            // If the same name was lifted multiple times (e.g. semantic dedup),
            // use the first; they'll produce the same CID anyway.
            map.entry(d.name.clone()).or_insert(ccid);
        }
        map
    };

    for (path_str, parsed_file) in &parsed_files {
        let edges = extract_call_edges_from_file(parsed_file, path_str, &contract_cid_map);
        report.call_edges.extend(edges);
    }

    report
}

/// Directory names that are never part of a Rust workspace's source tree.
/// Any directory whose name matches one of these is skipped entirely,
/// at any depth, on any host platform.
const IGNORED_DIRS: &[&str] = &[
    "target",
    ".git",
    "node_modules",
    "__pycache__",
    ".DS_Store",
    ".idea",
    ".vscode",
];

/// Normalize `path` to a canonical, relative POSIX path rooted at `root`.
///
/// Rules (per spec #120 Locus, "File field semantics"):
/// - The result is relative to `root` (no leading `/`, no drive letters).
/// - Separators are forward slashes only.
/// - Any leading `./` is stripped.
/// - Paths that escape `root` (via `..`) are rejected; `None` is returned.
fn to_relative_posix(root: &Path, path: &Path) -> Option<String> {
    // Use `strip_prefix` to get a path relative to root.  Both `root` and
    // `path` come from walkdir which emits descendants of root, so
    // `strip_prefix` will succeed unless something unusual happened.
    let rel = path.strip_prefix(root).ok()?;
    // Convert separators to forward slashes (defensive on Windows too).
    let posix = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/");
    // Reject empty (root itself) or any escaped path.
    if posix.is_empty() || posix.starts_with("..") {
        return None;
    }
    Some(posix)
}

/// Walk `root` for `.rs` source files, applying:
///  - Ignore-list filtering (build artifacts, VCS dirs, IDE noise).
///  - Deterministic lexicographic sort by relative POSIX path (byte order).
///
/// Returns a list of `(relative_posix_path, absolute_pathbuf)` pairs sorted
/// so that identical source trees produce identical output regardless of
/// host-filesystem readdir ordering (macOS APFS vs Linux ext4).
pub fn enumerate_rs_files(root: &Path) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    if !root.exists() {
        return out;
    }
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            // Skip ignored directories at any depth.
            let n = e.file_name().to_string_lossy();
            !IGNORED_DIRS.iter().any(|&ig| n == ig)
        })
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            if let Some(ext) = entry.path().extension() {
                if ext == "rs" {
                    if let Some(rel) = to_relative_posix(root, entry.path()) {
                        out.push((rel, entry.path().to_path_buf()));
                    }
                }
            }
        }
    }
    // Sort lexicographically by the relative POSIX path (raw byte order,
    // locale-independent) so walk order is deterministic across platforms.
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[derive(Debug)]
pub struct MintOutput {
    pub bytes: Vec<u8>,
    pub cid: String,
    /// Signer-independent trust anchor per spec #94.
    /// contractSetCid = blake3-512(JCS(<sorted contractCids>))
    /// where contractCids are content-only hashes:
    ///   contractCid = blake3-512(JCS({name, outBinding, pre?, post?, inv?}))
    pub contract_set_cid: String,
    pub member_count: usize,
    /// Map from contract name to its minted memento CID. Names that
    /// collide on identical canonical IR (semantic dedup) collapse to
    /// one entry; names that collide on different IR error out.
    pub contract_cids: BTreeMap<String, String>,
    pub deduplicated: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum LiftMintError {
    #[error("mint_contract: {0}")]
    Mint(String),
    #[error("name collision on different IR: contract `{0}` lifted twice with different bodies")]
    NameCollisionDifferentIr(String),
}

/// Coalesce decls that share a name into one. A producer callsite asserted
/// about in multiple places (e.g. a let-bound `.expect()` used in two
/// `assert!`s) is lifted as several decls with the same name but different
/// invariants. The honest single memento for that callsite carries the
/// CONJUNCTION of every fact asserted about it. This is computation over the
/// lifted data — the substrate's job, post-RPC — not the language kit's.
///
/// Exact-duplicate operands collapse (`a ∧ a ≡ a`). A contradiction
/// (`a ∧ ¬a`) is NOT masked here: it rides into the contract as an
/// unsatisfiable invariant and surfaces at prove/verify. That diagnostic is
/// the point of the system, caught at the proving layer — not papered over by
/// a naming-time error that also false-positived on compatible facts.
fn coalesce_decls_by_name(decls: &[ContractDecl]) -> (Vec<ContractDecl>, usize) {
    let mut order: Vec<String> = Vec::new();
    let mut grouped: BTreeMap<String, ContractDecl> = BTreeMap::new();
    // A same-name decl carrying IDENTICAL facts is a true duplicate (no new
    // information) and counts toward the dedup receipt. A same-name decl with
    // DISTINCT facts is conjoined, not deduplicated — it adds a fact (possibly
    // a contradictory one, which the solver will catch).
    let mut deduplicated = 0usize;
    for d in decls {
        if let Some(acc) = grouped.get_mut(&d.name) {
            let identical = format!("{:?}", acc.pre) == format!("{:?}", d.pre)
                && format!("{:?}", acc.post) == format!("{:?}", d.post)
                && format!("{:?}", acc.inv) == format!("{:?}", d.inv);
            if identical {
                deduplicated += 1;
            } else {
                acc.pre = conjoin_formula(acc.pre.take(), d.pre.clone());
                acc.post = conjoin_formula(acc.post.take(), d.post.clone());
                acc.inv = conjoin_formula(acc.inv.take(), d.inv.clone());
            }
        } else {
            order.push(d.name.clone());
            grouped.insert(d.name.clone(), d.clone());
        }
    }
    let out = order
        .into_iter()
        .filter_map(|name| grouped.remove(&name))
        .collect();
    (out, deduplicated)
}

/// Conjoin two optional formulas into one `and` connective, flattening nested
/// `and`s and dropping exact-duplicate operands (compared by canonical Debug
/// form, since `Formula` has no `PartialEq`). Distinct operands — including a
/// fact and its negation — are all preserved for the prover.
fn conjoin_formula(a: Option<Rc<Formula>>, b: Option<Rc<Formula>>) -> Option<Rc<Formula>> {
    let (a, b) = match (a, b) {
        (None, None) => return None,
        (Some(x), None) | (None, Some(x)) => return Some(x),
        (Some(x), Some(y)) => (x, y),
    };
    let mut operands: Vec<Rc<Formula>> = Vec::new();
    for f in [a, b] {
        match &*f {
            Formula::Connective { kind, operands: inner } if kind == "and" => {
                for op in inner {
                    push_unique_operand(&mut operands, op.clone());
                }
            }
            _ => push_unique_operand(&mut operands, f.clone()),
        }
    }
    match operands.len() {
        0 => None,
        1 => operands.into_iter().next(),
        _ => Some(Rc::new(Formula::Connective {
            kind: "and".to_string(),
            operands,
        })),
    }
}

fn push_unique_operand(operands: &mut Vec<Rc<Formula>>, candidate: Rc<Formula>) {
    let key = format!("{candidate:?}");
    if !operands.iter().any(|existing| format!("{existing:?}") == key) {
        operands.push(candidate);
    }
}

/// Mint each lifted ContractDecl as a signed memento and bundle into a
/// single `.proof` catalog. CONTENT-ADDRESSED DEDUP: two decls whose
/// canonical IR encodes to the same byte string mint to the same CID and
/// collapse to one member. Decls that share a NAME are coalesced first
/// (`coalesce_decls_by_name`) so every fact about a producer callsite lives in
/// one memento; the residual same-name guard below is then a defensive
/// tripwire that should never fire.
pub fn mint_proof(decls: &[ContractDecl], opts: &LiftOptions) -> Result<MintOutput, LiftMintError> {
    let mut members: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut contract_cids: BTreeMap<String, String> = BTreeMap::new();
    // Signer-independent content CIDs for contractSetCid computation (spec #94).
    let mut content_cids: Vec<String> = Vec::new();
    // Coalesce same-named decls (computation over the lifted data) before
    // minting, so each producer callsite yields one memento carrying the
    // conjunction of every fact asserted about it. The coalesce reports the
    // true-duplicate count (identical facts under one name); the mint loop
    // below adds content-CID collapses (identical canonical bytes under
    // different names).
    let (coalesced, mut deduplicated) = coalesce_decls_by_name(decls);

    for d in &coalesced {
        let args = MintContractArgs {
            formals: Vec::new(),
            emit_empty_formals: false,
            formal_sorts: Vec::new(),
            contract_name: d.name.clone(),
            pre: d.pre.as_deref().map(formula_to_value),
            post: d.post.as_deref().map(formula_to_value),
            inv: d.inv.as_deref().map(formula_to_value),
            out_binding: d.out_binding.clone(),
            produced_by: opts.produced_by.clone(),
            produced_at: opts.produced_at.clone(),
            input_cids: vec![],
            authoring: Authoring::Lift {
                lifter: opts.lifter.clone(),
                evidence: format!("lifted from `{}` annotations", d.name),
                source_cid: None,
            },
            signer_seed: opts.signer_seed,
        };
        // Compute signer-independent content CID BEFORE minting (spec #94).
        let ccid = compute_contract_cid(&args);
        let m = mint_contract(&args).map_err(|e| LiftMintError::Mint(e.to_string()))?;

        if let Some(prev_cid) = contract_cids.get(&d.name) {
            if prev_cid == &m.cid {
                // Same name, identical IR: semantic dedup.
                deduplicated += 1;
                continue;
            } else {
                return Err(LiftMintError::NameCollisionDifferentIr(d.name.clone()));
            }
        }

        content_cids.push(ccid);
        contract_cids.insert(d.name.clone(), m.cid.clone());
        // If the CID itself already exists (different name, same IR),
        // members map collapses on insert; count as dedup.
        if members.contains_key(&m.cid) {
            deduplicated += 1;
        } else {
            members.insert(m.cid, m.canonical_bytes);
        }
    }

    // Compute contractSetCid per spec #94 §1.
    let contract_set_cid = compute_contract_set_cid(content_cids);

    let signer_pubkey = ed25519_pubkey_string(&opts.signer_seed);
    let signer_cid = blake3_512_of(signer_pubkey.as_bytes());
    let proof_input = ProofEnvelopeInput {
        name: opts.catalog_name.clone(),
        version: opts.catalog_version.clone(),
        binary_cid: None,
        metadata: None,
        members: members.clone(),
        signer_cid,
        signer_seed: opts.signer_seed,
        declared_at: opts.produced_at.clone(),
    };
    let built = build_proof_envelope(&proof_input);

    Ok(MintOutput {
        bytes: built.bytes,
        cid: built.cid,
        contract_set_cid,
        member_count: members.len(),
        contract_cids,
        deduplicated,
    })
}

/// Convenience: walk -> lift -> mint -> write `<out_dir>/<cid>.proof`.
pub fn lift_and_mint(
    workspace_root: &Path,
    out_dir: &Path,
    opts: &LiftOptions,
) -> Result<(LiftReport, MintOutput, PathBuf), String> {
    let report = lift_path(workspace_root);
    if report.decls.is_empty() {
        return Err("no liftable contracts found in workspace".into());
    }
    let minted = mint_proof(&report.decls, opts).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(out_dir).map_err(|e| format!("create_dir_all: {e}"))?;
    let path = out_dir.join(format!("{}.proof", minted.cid));
    std::fs::write(&path, &minted.bytes).map_err(|e| format!("write: {e}"))?;
    Ok((report, minted, path))
}

// ---------------------------------------------------------------------------
// CLI shared between the two binaries.
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct CliFlags {
    pub workspace: Option<PathBuf>,
    pub target_dir: Option<PathBuf>,
    pub quiet: bool,
    pub rpc: bool,
}

pub fn parse_cli_flags(args: impl IntoIterator<Item = String>) -> CliFlags {
    let mut flags = CliFlags::default();
    let mut iter = args.into_iter();
    while let Some(a) = iter.next() {
        match a.as_str() {
            "--workspace" | "-w" => {
                flags.workspace = iter.next().map(PathBuf::from);
            }
            "--target-dir" | "-o" => {
                flags.target_dir = iter.next().map(PathBuf::from);
            }
            "--quiet" | "-q" => flags.quiet = true,
            "--rpc" => flags.rpc = true,
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            _ => {
                // Cargo passes the subcommand name as argv[1] when
                // invoked as `cargo provekit-lift ...`. Strip it.
                if a == "provekit-lift" {
                    continue;
                }
                eprintln!("provekit-lift: unrecognized argument: {a}");
                eprintln!("provekit-lift: try --help");
                std::process::exit(2);
            }
        }
    }
    flags
}

fn print_help() {
    println!(
        "provekit-lift: promote existing Rust annotations to signed contracts.\n\n\
         USAGE:\n  \
           cargo provekit-lift [--workspace <dir>] [--target-dir <dir>] [--quiet]\n  \
           provekit-lift     [--workspace <dir>] [--target-dir <dir>] [--quiet]\n\n\
         FLAGS:\n  \
           --workspace <dir>   Workspace root to walk. Default: current directory.\n  \
           --target-dir <dir>  Output directory. Default: <workspace>/target/release.\n  \
           --quiet             Suppress per-adapter summary lines.\n  \
           --rpc               Speak JSON-RPC over stdio (plugin mode).\n  \
           --help              Show this help.\n\n\
         POSITIONING:\n  \
           ProvekIt does NOT compete with proptest, contracts, kani, prusti,\n  \
           hypothesis-py, deal-py, bean-validation-java, zod-ts. It sits\n  \
           BENEATH them. We promote what you already have to content-addressed\n  \
           signed contracts. The macros in provekit-macros are the fallback\n  \
           for greenfield code."
    );
}

/// JSON-RPC plugin mode. Speaks NDJSON over stdio.
fn run_rpc_mode() -> i32 {
    use std::io::{BufRead, Write};
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let req: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let resp = serde_json::json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":format!("parse error: {e}")}});
                let _ = writeln!(stdout, "{resp}");
                continue;
            }
        };
        let id = req.get("id").cloned().unwrap_or(serde_json::Value::Null);
        let method = req.get("method").and_then(|v| v.as_str()).unwrap_or("");
        match method {
            "initialize" => {
                let resp = serde_json::json!({"jsonrpc":"2.0","id":id,"result":{"name":"provekit-lift","version":"1.0","capabilities":[]}});
                let _ = writeln!(stdout, "{resp}");
            }
            "lift" => {
                let params = req
                    .get("params")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({}));
                // workspace_root: prefer RPC param so the cmd_mint
                // lift-plugin protocol routes correctly (params are
                // authoritative per pep/1.7.0); fall back to CWD for
                // direct CLI use.
                let workspace = params
                    .get("workspace_root")
                    .and_then(|v| v.as_str())
                    .map(PathBuf::from)
                    .unwrap_or_else(|| {
                        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
                    });
                // options.emit selects the response shape. Default
                // "proof-envelope" preserves backward compat with the
                // self-minted path. "ir-document" emits raw contract
                // mementos so this plugin composes with sibling plugins
                // (walk_rpc for sugar/refuse) when cmd_mint runs N
                // plugins from a `[[plugins]]` config and merges their
                // ir-documents into one envelope.
                let emit = params
                    .get("options")
                    .and_then(|o| o.get("emit"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("proof-envelope")
                    .to_string();
                let opts = LiftOptions::default();
                if emit == "ir-document" {
                    let report = lift_path(&workspace);
                    // Content-addressed names (CID suffix) make
                    // duplicates safe — same name = same canonical IR =
                    // same minted memento. Dedup here so downstream
                    // consumers (cmd_mint's envelope minter) see a
                    // collision-free ir-document; the substrate's
                    // mint_proof primitive does the same thing one
                    // layer down, we surface it at the IR layer for
                    // multi-plugin merge correctness.
                    let mut seen: std::collections::HashSet<String> =
                        std::collections::HashSet::new();
                    let mut ir: Vec<serde_json::Value> = Vec::new();
                    for decl in &report.decls {
                        let entry = contract_decl_to_memento(decl);
                        let name = entry
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        if seen.insert(name) {
                            ir.push(entry);
                        }
                    }
                    let diagnostics: Vec<serde_json::Value> = report
                        .parse_errors
                        .iter()
                        .map(|(path, err)| {
                            serde_json::json!({
                                "severity": "warning",
                                "message": format!("parse {path}: {err}"),
                            })
                        })
                        .collect();
                    let resp = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "kind": "ir-document",
                            "ir": ir,
                            "diagnostics": diagnostics,
                        }
                    });
                    let _ = writeln!(stdout, "{resp}");
                } else {
                    let out_dir = workspace.join("target").join("release");
                    match lift_and_mint(&workspace, &out_dir, &opts) {
                        Ok((_report, minted, path)) => {
                            let bytes = std::fs::read(&path).unwrap_or_default();
                            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                            let resp = serde_json::json!({"jsonrpc":"2.0","id":id,"result":{"kind":"proof-envelope","filename_cid":minted.cid,"contract_set_cid":minted.contract_set_cid,"bytes_base64":b64}});
                            let _ = writeln!(stdout, "{resp}");
                        }
                        Err(e) => {
                            let resp = serde_json::json!({"jsonrpc":"2.0","id":id,"error":{"code":-32603,"message":format!("lift failed: {e}")}});
                            let _ = writeln!(stdout, "{resp}");
                        }
                    }
                }
            }
            "shutdown" => {
                let resp = serde_json::json!({"jsonrpc":"2.0","id":id,"result":null});
                let _ = writeln!(stdout, "{resp}");
                return 0;
            }
            _ => {
                let resp = serde_json::json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":format!("unknown method: {method}")}});
                let _ = writeln!(stdout, "{resp}");
            }
        }
    }
    0
}

/// Serialize a `ContractDecl` as a `kind: "contract"` JSON memento in
/// the canonical-IR shape `cmd_mint`'s ir-document consumer expects.
/// The contract's `name` is content-addressed by appending the
/// formula-set CID — byte-identical formulas at the same name collapse
/// safely; different formulas survive as distinct entries. This is the
/// same dedup primitive `mint_proof` already applies internally; we
/// surface it at the IR layer so multi-plugin merges in `cmd_mint`
/// remain content-honest before envelope minting.
fn contract_decl_to_memento(decl: &ContractDecl) -> serde_json::Value {
    use provekit_canonicalizer::encode_jcs;
    fn formula_pair(
        f: Option<&provekit_ir_symbolic::Formula>,
    ) -> (Option<serde_json::Value>, String) {
        match f {
            Some(formula) => {
                let cv = formula_to_value(formula);
                let jcs = encode_jcs(&cv);
                let cid = blake3_512_of(jcs.as_bytes());
                let value = serde_json::from_str(&jcs).unwrap_or(serde_json::Value::Null);
                (Some(value), cid)
            }
            None => (None, String::new()),
        }
    }
    let (inv_value, inv_cid) = formula_pair(decl.inv.as_deref());
    let (pre_value, pre_cid) = formula_pair(decl.pre.as_deref());
    let (post_value, post_cid) = formula_pair(decl.post.as_deref());
    let content_cid = blake3_512_of(format!("{inv_cid}|{pre_cid}|{post_cid}").as_bytes());
    let name = format!("{}#{}", decl.name, content_cid);
    let mut entry = serde_json::json!({
        "kind": "contract",
        "name": name,
        "outBinding": decl.out_binding,
    });
    if let Some(v) = inv_value {
        entry["inv"] = v;
    }
    if let Some(v) = pre_value {
        entry["pre"] = v;
    }
    if let Some(v) = post_value {
        entry["post"] = v;
    }
    entry
}

/// Entry point shared by both bin targets. Returns a process exit code.
pub fn run_cli(flags: CliFlags) -> i32 {
    if flags.rpc {
        return run_rpc_mode();
    }
    let workspace = flags.workspace.unwrap_or_else(|| PathBuf::from("."));
    let out_dir = flags
        .target_dir
        .unwrap_or_else(|| workspace.join("target").join("release"));

    let opts = LiftOptions::default();
    match lift_and_mint(&workspace, &out_dir, &opts) {
        Ok((report, minted, path)) => {
            if !flags.quiet {
                println!("provekit-lift: scanned {} .rs files", report.files_scanned);
                for ar in &report.adapter_reports {
                    println!(
                        "  adapter `{}`: seen {}, lifted {}, skipped {}",
                        ar.adapter,
                        ar.seen,
                        ar.lifted,
                        ar.warnings.len()
                    );
                }
                if minted.deduplicated > 0 {
                    println!(
                        "  dedup: {} contracts collapsed by content address",
                        minted.deduplicated
                    );
                }
                println!(
                    "provekit-lift: wrote {} ({} members)",
                    path.display(),
                    minted.member_count
                );
                println!("provekit-lift: cid = {}", minted.cid);
            } else {
                println!("{}", minted.cid);
            }
            0
        }
        Err(e) => {
            eprintln!("provekit-lift: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tempdir() -> tempdir_compat::TempDir {
        tempdir_compat::TempDir::new("provekit-lift-test").unwrap()
    }

    /// Smallest possible "test workspace": one .rs file with one
    /// proptest block and one contracts-annotated function, plus a
    /// duplicate proptest block in a second file (dedup case).
    fn write_fixture(dir: &Path) {
        let a = dir.join("a.rs");
        let mut f = std::fs::File::create(&a).unwrap();
        writeln!(
            f,
            r#"
proptest! {{
    #[test]
    fn answer_is_42(x: i64) {{
        prop_assert_eq!(x, 42);
    }}
}}

#[requires(x > 0)]
#[ensures(ret >= 0)]
fn sqrt(x: i64) -> i64 {{ x }}
"#
        )
        .unwrap();
        let b = dir.join("b.rs");
        let mut f = std::fs::File::create(&b).unwrap();
        // Same property, expressed identically: should dedup.
        writeln!(
            f,
            r#"
proptest! {{
    #[test]
    fn answer_is_42(x: i64) {{
        prop_assert_eq!(x, 42);
    }}
}}
"#
        )
        .unwrap();
    }

    #[test]
    fn end_to_end_walk_lift_mint_dedup() {
        let td = tempdir();
        write_fixture(td.path());
        let opts = LiftOptions::default();
        let (report, minted, path) =
            lift_and_mint(td.path(), td.path(), &opts).expect("lift_and_mint");
        // 2 proptest fns seen across the two files, 1 sqrt fn.
        let proptest = report
            .adapter_reports
            .iter()
            .find(|a| a.adapter == "proptest")
            .unwrap();
        let contracts = report
            .adapter_reports
            .iter()
            .find(|a| a.adapter == "contracts")
            .unwrap();
        assert_eq!(proptest.seen, 2);
        assert_eq!(proptest.lifted, 2);
        assert_eq!(contracts.lifted, 1);
        // Dedup: two `answer_is_42` collapse to one minted member; sqrt
        // adds one more = 2 members.
        assert_eq!(minted.member_count, 2, "expected dedup; got {minted:?}");
        assert!(path.exists());
        assert!(minted.cid.starts_with("blake3-512:"));
    }

    #[test]
    fn cli_flags_parse() {
        let flags = parse_cli_flags(
            [
                "--workspace".into(),
                "/a".into(),
                "--target-dir".into(),
                "/b".into(),
            ]
            .into_iter()
            .collect::<Vec<String>>(),
        );
        assert_eq!(flags.workspace.as_deref(), Some(Path::new("/a")));
        assert_eq!(flags.target_dir.as_deref(), Some(Path::new("/b")));
    }

    #[test]
    fn cargo_subcommand_arg_is_stripped() {
        // When invoked as `cargo provekit-lift --workspace /a`, Cargo
        // calls `cargo-provekit-lift` with argv = ["provekit-lift",
        // "--workspace", "/a"]. parse_cli_flags must skip "provekit-lift".
        let flags = parse_cli_flags(
            ["provekit-lift".into(), "--workspace".into(), "/a".into()]
                .into_iter()
                .collect::<Vec<String>>(),
        );
        assert_eq!(flags.workspace.as_deref(), Some(Path::new("/a")));
    }

    // -----------------------------------------------------------------------
    // Determinism tests (spec #120 §"File field semantics" + §11 manifesto).
    //
    // Two instances of the SAME source tree at DIFFERENT absolute paths MUST
    // produce byte-identical contractSetCid.  This is the empirical gate for
    // cross-platform federated trust: different machines will always have
    // different absolute paths, but the relative POSIX path within the project
    // root is identical.
    // -----------------------------------------------------------------------

    /// Write a richer fixture: two .rs files in a nested sub-directory, a
    /// target/ directory with a dummy file that must NOT be lifted, and a
    /// .DS_Store file that must NOT be lifted.
    fn write_determinism_fixture(dir: &Path) {
        // Create nested source layout.
        let sub = dir.join("src").join("nested");
        std::fs::create_dir_all(&sub).unwrap();

        let mut f = std::fs::File::create(sub.join("alpha.rs")).unwrap();
        writeln!(
            f,
            r#"
#[requires(x > 0)]
#[ensures(ret > 0)]
fn positive(x: i64) -> i64 {{ x }}
"#
        )
        .unwrap();

        let mut f = std::fs::File::create(sub.join("beta.rs")).unwrap();
        writeln!(
            f,
            r#"
#[requires(n >= 0)]
#[ensures(ret >= 0)]
fn nonneg(n: i64) -> i64 {{ n }}
"#
        )
        .unwrap();

        // A top-level file too.
        let mut f = std::fs::File::create(dir.join("lib.rs")).unwrap();
        writeln!(
            f,
            r#"
#[requires(a != 0)]
#[ensures(ret != 0)]
fn nonzero(a: i64) -> i64 {{ a }}
"#
        )
        .unwrap();

        // Build artifact directory: must be filtered.
        let target = dir.join("target").join("release");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("artifact.rs"), b"fn build_artifact() {}").unwrap();

        // macOS noise file: must be filtered.
        std::fs::write(dir.join(".DS_Store"), b"").unwrap();
    }

    /// Empirical cross-machine determinism test.
    ///
    /// Lifts the same fixture from TWO different absolute roots (simulating
    /// two machines with different directory prefixes) and asserts that
    /// `contract_set_cid` and bundle `cid` are byte-identical.
    #[test]
    fn contract_set_cid_is_identical_across_different_absolute_roots() {
        let td1 = tempdir_compat::TempDir::new("provekit-det-machine1").unwrap();
        let td2 = tempdir_compat::TempDir::new("provekit-det-machine2").unwrap();

        write_determinism_fixture(td1.path());
        write_determinism_fixture(td2.path());

        // Sanity: the two roots must NOT be the same directory.
        assert_ne!(
            td1.path(),
            td2.path(),
            "test requires two distinct temp dirs"
        );

        let opts = LiftOptions::default();

        let report1 = lift_path(td1.path());
        let report2 = lift_path(td2.path());

        // Both should have lifted the same number of contracts.
        assert_eq!(
            report1.decls.len(),
            report2.decls.len(),
            "declaration count must match across roots"
        );

        let mint1 = mint_proof(&report1.decls, &opts).expect("mint1");
        let mint2 = mint_proof(&report2.decls, &opts).expect("mint2");

        assert_eq!(
            mint1.contract_set_cid, mint2.contract_set_cid,
            "contractSetCid must be byte-identical across different absolute roots \
             (simulates cross-machine CID stability)"
        );
        assert_eq!(
            mint1.cid, mint2.cid,
            "bundle CID must be byte-identical across different absolute roots"
        );
    }

    /// Assert that target/ and .DS_Store artifacts are NOT in the lifted set.
    #[test]
    fn target_dir_and_ds_store_are_excluded() {
        let td = tempdir_compat::TempDir::new("provekit-det-ignore").unwrap();
        write_determinism_fixture(td.path());

        let report = lift_path(td.path());

        // Only 3 files (lib.rs, alpha.rs, beta.rs) should have been scanned;
        // target/release/artifact.rs and .DS_Store must be excluded.
        assert_eq!(
            report.files_scanned, 3,
            "expected 3 .rs files scanned; got {}. target/ or .DS_Store pollution?",
            report.files_scanned
        );
    }

    /// Assert that file paths embedded in call-edge loci are relative POSIX
    /// paths, not absolute paths.
    #[test]
    fn call_edge_loci_use_relative_posix_paths() {
        let td = tempdir_compat::TempDir::new("provekit-det-locus").unwrap();
        write_determinism_fixture(td.path());

        let report = lift_path(td.path());

        for edge in &report.call_edges {
            let file = &edge.call_site_locus.file;
            assert!(
                !file.starts_with('/'),
                "call-edge locus file must be relative, got absolute: {file}"
            );
            // No Windows drive letters.
            assert!(
                !file.chars().nth(1).map_or(false, |c| c == ':'),
                "call-edge locus file must not contain drive letter: {file}"
            );
            // No backslashes.
            assert!(
                !file.contains('\\'),
                "call-edge locus file must use forward slashes only: {file}"
            );
        }

        // enumerate_rs_files returns (rel_posix, abs): verify the posix strings too.
        let entries = enumerate_rs_files(td.path());
        for (rel, _) in &entries {
            assert!(
                !rel.starts_with('/'),
                "enumerate_rs_files returned absolute path: {rel}"
            );
            assert!(
                !rel.contains('\\'),
                "enumerate_rs_files returned backslash: {rel}"
            );
        }
    }

    /// Assert enumerate_rs_files output is sorted lexicographically.
    #[test]
    fn enumerate_rs_files_is_sorted() {
        let td = tempdir_compat::TempDir::new("provekit-det-sort").unwrap();
        write_determinism_fixture(td.path());

        let entries = enumerate_rs_files(td.path());
        let paths: Vec<&str> = entries.iter().map(|(r, _)| r.as_str()).collect();
        let mut sorted = paths.clone();
        sorted.sort();
        assert_eq!(
            paths, sorted,
            "enumerate_rs_files must return lexicographically sorted paths"
        );
    }
}

#[cfg(test)]
mod tempdir_compat {
    use std::path::{Path, PathBuf};

    pub struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        pub fn new(prefix: &str) -> std::io::Result<Self> {
            let base = std::env::temp_dir();
            // Use nanos + pid + counter for a non-colliding name.
            use std::sync::atomic::{AtomicU64, Ordering};
            static CTR: AtomicU64 = AtomicU64::new(0);
            let n = CTR.fetch_add(1, Ordering::Relaxed);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let path = base.join(format!("{prefix}-{}-{}-{}", std::process::id(), nanos, n));
            std::fs::create_dir_all(&path)?;
            Ok(Self { path })
        }
        pub fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}
