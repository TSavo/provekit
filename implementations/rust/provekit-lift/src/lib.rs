// SPDX-License-Identifier: Apache-2.0
//
// provekit-lift — workspace lift toolchain.
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

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use provekit_canonicalizer::blake3_512_of;
use provekit_claim_envelope::{mint_contract, Authoring, MintContractArgs};
use provekit_ir_symbolic::{serialize::formula_to_value, ContractDecl};
use provekit_proof_envelope::{build_proof_envelope, ed25519_pubkey_string, Ed25519Seed, ProofEnvelopeInput};

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

    for path in enumerate_rs_files(root) {
        report.files_scanned += 1;
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                report
                    .parse_errors
                    .push((path.display().to_string(), format!("read: {e}")));
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
                    .push((path.display().to_string(), format!("parse: {e}")));
                continue;
            }
        };
        let path_str = path.display().to_string();

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

        // Adapter: rust-tests (#[test] / #[tokio::test] -> per-assertion mementos).
        let rt_out = adapter_rust_tests::lift_file(&file, &path_str);
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
    report
}

fn enumerate_rs_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if !root.exists() {
        return out;
    }
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            // Skip target/ and .git/ unconditionally.
            let n = e.file_name().to_string_lossy();
            !(n == "target" || n == ".git" || n == "node_modules")
        })
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            if let Some(ext) = entry.path().extension() {
                if ext == "rs" {
                    out.push(entry.path().to_path_buf());
                }
            }
        }
    }
    out
}

#[derive(Debug)]
pub struct MintOutput {
    pub bytes: Vec<u8>,
    pub cid: String,
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

/// Mint each lifted ContractDecl as a signed memento and bundle into a
/// single `.proof` catalog. CONTENT-ADDRESSED DEDUP: two decls whose
/// canonical IR encodes to the same byte string mint to the same CID
/// and collapse to one member. Names that disagree on the IR fail
/// loud (the lattice should not silently accept contradiction).
pub fn mint_proof(
    decls: &[ContractDecl],
    opts: &LiftOptions,
) -> Result<MintOutput, LiftMintError> {
    let mut members: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut contract_cids: BTreeMap<String, String> = BTreeMap::new();
    let mut deduplicated = 0usize;

    for d in decls {
        let args = MintContractArgs {
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
        let m = mint_contract(&args).map_err(|e| LiftMintError::Mint(e.to_string()))?;

        if let Some(prev_cid) = contract_cids.get(&d.name) {
            if prev_cid == &m.cid {
                // Same name, identical IR — semantic dedup.
                deduplicated += 1;
                continue;
            } else {
                return Err(LiftMintError::NameCollisionDifferentIr(d.name.clone()));
            }
        }

        contract_cids.insert(d.name.clone(), m.cid.clone());
        // If the CID itself already exists (different name, same IR),
        // members map collapses on insert; count as dedup.
        if members.contains_key(&m.cid) {
            deduplicated += 1;
        } else {
            members.insert(m.cid, m.canonical_bytes);
        }
    }

    let signer_pubkey = ed25519_pubkey_string(&opts.signer_seed);
    let signer_cid = blake3_512_of(signer_pubkey.as_bytes());
    let proof_input = ProofEnvelopeInput {
        name: opts.catalog_name.clone(),
        version: opts.catalog_version.clone(),
        members: members.clone(),
        signer_cid,
        signer_seed: opts.signer_seed,
        declared_at: opts.produced_at.clone(),
    };
    let built = build_proof_envelope(&proof_input);

    Ok(MintOutput {
        bytes: built.bytes,
        cid: built.cid,
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
        "provekit-lift — promote existing Rust annotations to signed contracts.\n\n\
         USAGE:\n  \
           cargo provekit-lift [--workspace <dir>] [--target-dir <dir>] [--quiet]\n  \
           provekit-lift     [--workspace <dir>] [--target-dir <dir>] [--quiet]\n\n\
         FLAGS:\n  \
           --workspace <dir>   Workspace root to walk. Default: current directory.\n  \
           --target-dir <dir>  Output directory. Default: <workspace>/target/release.\n  \
           --quiet             Suppress per-adapter summary lines.\n  \
           --help              Show this help.\n\n\
         POSITIONING:\n  \
           ProvekIt does NOT compete with proptest, contracts, kani, prusti,\n  \
           hypothesis-py, deal-py, bean-validation-java, zod-ts. It sits\n  \
           BENEATH them. We promote what you already have to content-addressed\n  \
           signed contracts. The macros in provekit-macros are the fallback\n  \
           for greenfield code."
    );
}

/// Entry point shared by both bin targets. Returns a process exit code.
pub fn run_cli(flags: CliFlags) -> i32 {
    let workspace = flags.workspace.unwrap_or_else(|| PathBuf::from("."));
    let out_dir = flags
        .target_dir
        .unwrap_or_else(|| workspace.join("target").join("release"));

    let opts = LiftOptions::default();
    match lift_and_mint(&workspace, &out_dir, &opts) {
        Ok((report, minted, path)) => {
            if !flags.quiet {
                println!(
                    "provekit-lift: scanned {} .rs files",
                    report.files_scanned
                );
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
        // Same property, expressed identically — should dedup.
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
            ["--workspace".into(), "/a".into(), "--target-dir".into(), "/b".into()]
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
}

// ---------------------------------------------------------------------------
// Tiny tempdir shim so we don't pull a new workspace dep just for tests.
// ---------------------------------------------------------------------------

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
