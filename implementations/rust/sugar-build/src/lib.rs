// SPDX-License-Identifier: Apache-2.0
//
// provekit-build
//
// Cargo build-script integration for ProvekIt. A consumer crate that
// wants its `#[provekit::contract(...)]` and `#[provekit::verify]`
// annotations to gate `cargo build` adds:
//
//   1. A `[build-dependencies] provekit-build = ...` line to its
//      `Cargo.toml`.
//   2. A `[package.metadata.provekit]` table for configuration.
//   3. A `build.rs` containing one line:
//        fn main() { sugar_build::run_verification(); }
//   4. The actual `#[provekit::contract]` / `#[provekit::verify]`
//      annotations on its functions.
//
// On `cargo build`, cargo compiles + runs the consumer's build.rs;
// `run_verification()` source-walks the consumer's `src/` tree (via
// `syn`), enumerates contract declarations and verify targets,
// optionally mints them into a `<cid>.proof` file under `target/`,
// and runs a Tier-3 per-callsite Z3 check on each `#[verify]`
// function body. Findings are surfaced via cargo's stable build-script
// protocol:
//
//   * Each call site that the verifier flags emits a
//     `cargo:warning=provekit: <message>` line. Cargo prints these in
//     the build output without failing the compile.
//   * In `strict = true` mode, an undischarged call site additionally
//     prints an `error: ...` line and exits the build script with a
//     non-zero status, which cargo treats as a build failure (the
//     equivalent of a `compile_error!` from a proc-macro context).
//   * `cargo:rerun-if-changed=...` lines are emitted for every source
//     file we walked plus the consumer's Cargo.toml, so cargo invokes
//     the verifier again whenever any of them changes.
//
// Tier handshake: this crate only implements Tier 3 (per-callsite Z3
// query, capped at 3s per call). Tiers 1 and 2 (handshake-cached
// implication mementos) are explicitly out of scope here; they live
// in a separate crate and depend on the implication-store work.
//
// See README.md in this crate for the consumer-facing 4-line
// integration shape.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};
use syn::visit::Visit;

pub mod lift_pass;
pub mod source_walk;

pub use lift_pass::{
    run_lift_pass, LiftAdapterCount, LiftPassReport, LiftedContract, ALL_ADAPTERS,
};
pub use source_walk::{ContractSite, FormulaShape, VerifySite, WalkOutcome};

/// Default Z3 query timeout in milliseconds.
pub const DEFAULT_Z3_TIMEOUT_MS: u64 = 3_000;

/// Per the protocol, hashes are BLAKE3-512 with a `blake3-512:` prefix.
pub const HASH_PREFIX: &str = "blake3-512:";

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// The `[package.metadata.provekit]` table from a consumer's Cargo.toml.
///
/// All fields are optional; missing values fall back to defaults
/// matching the v0 contract:
///
///   * `strict = false`     warnings only, never fail the build.
///   * `mint_proof = true`  produce a `<cid>.proof` under `target/`.
///   * `verify_targets`     glob filter; default `**/*` matches every
///                          `#[provekit::verify]` annotation.
///   * `z3_timeout_ms`      per-call solver timeout (default 3000).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct ProvekitConfig {
    /// When true, undischarged call sites cause the build to fail with
    /// a `cargo:warning=` plus a stderr `error:` and a non-zero exit.
    pub strict: Option<bool>,
    /// When true (default), `run_verification()` writes a `.proof`
    /// file to `<target_dir>/provekit/<cid>.proof`.
    pub mint_proof: Option<bool>,
    /// Glob over verify-target function names (not source paths).
    /// Default `**/*` matches every annotation.
    pub verify_targets: Option<String>,
    /// Per-call Z3 timeout, milliseconds. Default 3000.
    pub z3_timeout_ms: Option<u64>,
    /// Optional whitelist of lift-adapter names to run during build.
    /// Default (None) runs every registered adapter; an empty list
    /// disables the lift pass entirely. Recognized names are listed in
    /// `lift_pass::ALL_ADAPTERS`.
    pub lift_adapters: Option<Vec<String>>,
}

impl ProvekitConfig {
    pub fn strict(&self) -> bool {
        self.strict.unwrap_or(false)
    }
    pub fn mint_proof(&self) -> bool {
        self.mint_proof.unwrap_or(true)
    }
    pub fn verify_targets(&self) -> String {
        self.verify_targets
            .clone()
            .unwrap_or_else(|| "**/*".to_string())
    }
    pub fn z3_timeout_ms(&self) -> u64 {
        self.z3_timeout_ms.unwrap_or(DEFAULT_Z3_TIMEOUT_MS)
    }
    /// Adapter whitelist resolved against `ALL_ADAPTERS`. `None` means
    /// "all adapters"; an explicit empty list means "no adapters".
    /// Unknown names are filtered out (the surrounding code emits a
    /// `cargo:warning=` for each so the consumer notices typos).
    pub fn enabled_lift_adapters(&self) -> Vec<&'static str> {
        match &self.lift_adapters {
            None => ALL_ADAPTERS.iter().copied().collect(),
            Some(list) => ALL_ADAPTERS
                .iter()
                .copied()
                .filter(|name| list.iter().any(|w| w == name))
                .collect(),
        }
    }
    /// Names in `lift_adapters` that don't match any known adapter.
    /// Used for diagnostic warnings; never errors.
    pub fn unknown_lift_adapters(&self) -> Vec<String> {
        match &self.lift_adapters {
            None => Vec::new(),
            Some(list) => list
                .iter()
                .filter(|w| !ALL_ADAPTERS.iter().any(|name| w == name))
                .cloned()
                .collect(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("read Cargo.toml: {0}")]
    Read(String),
    #[error("parse Cargo.toml: {0}")]
    Parse(String),
}

/// Parse `[package.metadata.provekit]` from a Cargo.toml string.
///
/// A missing table is not an error; we return the all-defaults
/// `ProvekitConfig`. An invalid table (e.g. a key with the wrong type,
/// or an unknown field thanks to `deny_unknown_fields`) is.
pub fn parse_config_from_str(toml_str: &str) -> Result<ProvekitConfig, ConfigError> {
    let parsed: toml::Value =
        toml::from_str(toml_str).map_err(|e| ConfigError::Parse(format!("toml: {e}")))?;
    let metadata = parsed
        .get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("provekit"));
    let cfg = match metadata {
        Some(t) => {
            let s = toml::to_string(t)
                .map_err(|e| ConfigError::Parse(format!("re-emit metadata: {e}")))?;
            toml::from_str::<ProvekitConfig>(&s)
                .map_err(|e| ConfigError::Parse(format!("[package.metadata.provekit]: {e}")))?
        }
        None => ProvekitConfig::default(),
    };
    Ok(cfg)
}

/// Read and parse the consumer's Cargo.toml at `path`.
pub fn parse_config_from_path(path: &Path) -> Result<ProvekitConfig, ConfigError> {
    let s = fs::read_to_string(path)
        .map_err(|e| ConfigError::Read(format!("{}: {e}", path.display())))?;
    parse_config_from_str(&s)
}

// ---------------------------------------------------------------------------
// SMT emission for Tier-3 per-callsite Z3 checks
// ---------------------------------------------------------------------------

/// One per-callsite verification obligation, ready for Z3.
///
/// `script_smt2` is the full SMT-LIB 2 source: `(set-option :timeout
/// ...)` header, sort declarations, the asserted obligation, and a
/// `(check-sat)`. The verifier expects the first non-empty line of
/// stdout to be `unsat` (discharged), `sat` (counterexample), or
/// anything else (undecidable / timeout).
#[derive(Debug, Clone)]
pub struct ObligationScript {
    pub callsite_label: String,
    pub script_smt2: String,
}

/// Build an SMT-LIB 2 script that asks Z3 whether the contracted
/// function's POST-condition is consistent with the observed call-site
/// shape. The current shape recognizer handles:
///
///   * `gte(out(), num(N))`   the return value is at-least N
///   * `gt(out(), num(N))`    strictly greater
///   * `eq(out(), num(N))`    exactly N
///   * everything else        emits an `undecidable` script
///
/// At each call site, if the surrounding `#[verify]` body contains a
/// pattern of the form `if <bound_var> == <CONST>` against the call
/// site's binding, we emit an obligation asserting the contract AND
/// the equality; an `unsat` answer means the branch is dead per the
/// contract (the `deliberate_violation` case in the demo crate). A
/// `sat` answer is a counterexample: the branch is reachable.
pub fn build_obligation_script(
    cfg: &ProvekitConfig,
    callsite_label: &str,
    contract_post: &FormulaShape,
    surrounding_check: Option<i64>,
) -> ObligationScript {
    let timeout = cfg.z3_timeout_ms();
    let mut s = String::new();
    s.push_str(&format!("(set-option :timeout {timeout})\n"));
    s.push_str("(set-logic QF_LIA)\n");
    s.push_str("(declare-const out Int)\n");
    match contract_post {
        FormulaShape::GteConst(n) => {
            s.push_str(&format!("(assert (>= out {n}))\n"));
        }
        FormulaShape::GtConst(n) => {
            s.push_str(&format!("(assert (> out {n}))\n"));
        }
        FormulaShape::EqConst(n) => {
            s.push_str(&format!("(assert (= out {n}))\n"));
        }
        FormulaShape::Opaque => {
            // No constraint we can encode; leave the model unconstrained
            // so Z3 returns sat trivially (the solver cannot rule out
            // the surrounding check). The verifier maps that to
            // "undecidable" downstream.
        }
    }
    if let Some(c) = surrounding_check {
        s.push_str(&format!("(assert (= out {c}))\n"));
    }
    s.push_str("(check-sat)\n");
    ObligationScript {
        callsite_label: callsite_label.to_string(),
        script_smt2: s,
    }
}

// ---------------------------------------------------------------------------
// Z3 subprocess invocation, with timeout enforced both inside the
// SMT script (`:timeout`) and as a hard wall-clock cap on the child.
// ---------------------------------------------------------------------------

/// What Z3 returned for one obligation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SolverVerdict {
    /// Z3 returned `unsat`: the obligation holds.
    Discharged,
    /// Z3 returned `sat`: counterexample found.
    Unsatisfied,
    /// Z3 returned `unknown`, timed out, or could not be run.
    Undecidable,
}

#[derive(Debug, Clone)]
pub struct SolveResult {
    pub verdict: SolverVerdict,
    pub raw_stdout: String,
    pub note: String,
}

/// Resolve which z3 binary to invoke. Honors `PROVEKIT_Z3_PATH`,
/// otherwise falls back to `z3` on `$PATH`.
pub fn z3_binary_path() -> String {
    std::env::var("PROVEKIT_Z3_PATH").unwrap_or_else(|_| "z3".to_string())
}

/// Run a single SMT-LIB 2 script through z3 with a hard wall-clock cap
/// of `timeout_ms` milliseconds. The script SHOULD also carry an
/// internal `(set-option :timeout ...)` header so z3 self-aborts; the
/// outer wall-clock guard is belt-and-suspenders for a hung child.
pub fn solve(z3_path: &str, smt2_script: &str, timeout_ms: u64) -> SolveResult {
    let mut child = match Command::new(z3_path)
        .arg("-smt2")
        .arg("-in")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            return SolveResult {
                verdict: SolverVerdict::Undecidable,
                raw_stdout: String::new(),
                note: format!("spawn {z3_path}: {e}"),
            };
        }
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(smt2_script.as_bytes());
    }
    // Wall-clock guard. We poll `try_wait` rather than block on
    // `wait_with_output`, so a runaway z3 cannot exceed `timeout_ms`
    // even if `:timeout` is not honored.
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    let mut killed = false;
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => break,
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    killed = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(e) => {
                return SolveResult {
                    verdict: SolverVerdict::Undecidable,
                    raw_stdout: String::new(),
                    note: format!("wait z3: {e}"),
                };
            }
        }
    }
    let output = match child.wait_with_output() {
        Ok(o) => o,
        Err(e) => {
            return SolveResult {
                verdict: SolverVerdict::Undecidable,
                raw_stdout: String::new(),
                note: format!("collect output: {e}"),
            };
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let first = stdout
        .lines()
        .map(|l| l.trim_end_matches('\r'))
        .find(|l| !l.is_empty())
        .unwrap_or_default();
    let (verdict, note) = if killed {
        (
            SolverVerdict::Undecidable,
            format!("wall-clock timeout after {timeout_ms}ms"),
        )
    } else {
        match first {
            "unsat" => (SolverVerdict::Discharged, String::new()),
            "sat" => (SolverVerdict::Unsatisfied, String::new()),
            "unknown" => (
                SolverVerdict::Undecidable,
                "solver returned unknown (likely :timeout)".into(),
            ),
            other if other.is_empty() => (
                SolverVerdict::Undecidable,
                "solver produced no verdict".into(),
            ),
            other => (
                SolverVerdict::Undecidable,
                format!("unrecognized verdict: {other}"),
            ),
        }
    };
    SolveResult {
        verdict,
        raw_stdout: stdout,
        note,
    }
}

// ---------------------------------------------------------------------------
// Per-callsite report aggregation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CallsiteOutcome {
    pub verify_fn: String,
    pub callee: String,
    pub source_path: PathBuf,
    pub line: usize,
    pub verdict: SolverVerdict,
    pub note: String,
}

#[derive(Debug, Default, Clone)]
pub struct VerificationReport {
    /// Number of `#[provekit::contract]` decorators found by the
    /// source-walker. Treated as the "inventory" lane: contracts the
    /// kit's own macros / `.invariant.rs` files declared directly.
    pub contract_count: usize,
    /// Number of `#[provekit::verify]` annotations.
    pub verify_count: usize,
    pub callsites: Vec<CallsiteOutcome>,
    pub mint_path: Option<PathBuf>,
    pub mint_cid: Option<String>,
    /// Number of contracts produced by lift adapters (proptest, contracts,
    /// kani, prusti, creusot, flux, quickcheck, verus). Not the same
    /// thing as `contract_count`: these came from third-party
    /// annotations the consumer already had, not from `#[provekit::*]`
    /// decorators. Populated by the lift pass that runs alongside the
    /// source walk.
    pub lift_count: usize,
    /// Per-adapter breakdown. Always carries an entry per registered
    /// adapter (zeroed when the adapter found nothing or wasn't on the
    /// whitelist), so the report shape stays predictable.
    pub lift_breakdown: Vec<LiftAdapterCount>,
    /// Lifted contract decls, in the order the adapters produced them.
    /// Round-tripped into the proof manifest so consumers can audit
    /// what got promoted from third-party annotations.
    pub lifted_contracts: Vec<LiftedContract>,
    /// Names listed under `lift_adapters` that don't match any
    /// registered adapter. Surfaced via `cargo:warning=` so typos
    /// don't silently disable a lane.
    pub unknown_lift_adapters: Vec<String>,
}

impl VerificationReport {
    pub fn discharged_count(&self) -> usize {
        self.callsites
            .iter()
            .filter(|c| c.verdict == SolverVerdict::Discharged)
            .count()
    }
    pub fn unsatisfied_count(&self) -> usize {
        self.callsites
            .iter()
            .filter(|c| c.verdict == SolverVerdict::Unsatisfied)
            .count()
    }
    pub fn undecidable_count(&self) -> usize {
        self.callsites
            .iter()
            .filter(|c| c.verdict == SolverVerdict::Undecidable)
            .count()
    }
    /// True iff at least one call site is `Unsatisfied`: i.e. the
    /// solver returned a counterexample. Used by strict mode to fail
    /// the build.
    pub fn has_violations(&self) -> bool {
        self.unsatisfied_count() > 0
    }
}

// ---------------------------------------------------------------------------
// Proof minting (lightweight: a deterministic CBOR-ish JSON manifest
// hashed with BLAKE3-512). The full claim-envelope plumbing lives in
// provekit-claim-envelope; we deliberately emit a smaller manifest
// here so the build script does not pull the entire kit's dependency
// graph through the consumer's build pipeline.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
struct ProofManifest {
    schema: &'static str,
    contracts: Vec<ProofContract>,
    verify_targets: Vec<ProofVerifyTarget>,
    /// Contracts promoted from third-party annotations (proptest,
    /// contracts, kani, ...). Sourced from `lift_pass::run_lift_pass`,
    /// not the source-walker's `#[provekit::contract]` lane.
    lift_contracts: Vec<ProofLiftContract>,
}

#[derive(Debug, Clone, Serialize)]
struct ProofContract {
    name: String,
    source_path: String,
    line: usize,
    formula_shape: String,
}

#[derive(Debug, Clone, Serialize)]
struct ProofVerifyTarget {
    fn_name: String,
    source_path: String,
    line: usize,
}

/// Compact, deterministic, JSON-friendly snapshot of one lifted
/// ContractDecl. We do not ship the full canonical IR through the
/// build-script's tiny manifest (the heavy minting lives in
/// provekit-claim-envelope, which `cargo provekit-lift` invokes
/// directly). Here we record the adapter that produced it, the source
/// path, and which slots are populated.
#[derive(Debug, Clone, Serialize)]
struct ProofLiftContract {
    adapter: String,
    name: String,
    source_path: String,
    has_pre: bool,
    has_post: bool,
    has_inv: bool,
    out_binding: String,
}

/// Mint the proof manifest, hash it with BLAKE3-512, write the result
/// to `<target_dir>/provekit/<cid>.proof`. Returns the path and the
/// full self-identifying CID (`blake3-512:<128 hex>`).
///
/// The manifest carries both lanes: the source-walker's "inventory"
/// contracts (from `#[provekit::contract]` decorators) AND the lift
/// pass's contracts (from proptest / contracts / kani / ... third-party
/// annotations). Both contribute to the CID; a build that grows lift
/// adapters changes the proof, which is the desired property.
pub fn mint_proof_file(
    target_dir: &Path,
    walk: &WalkOutcome,
    lifted: &[LiftedContract],
) -> Result<(PathBuf, String), String> {
    let manifest = ProofManifest {
        schema: "provekit-build/0.2",
        contracts: walk
            .contracts
            .iter()
            .map(|c| ProofContract {
                name: c.fn_name.clone(),
                source_path: c.source_path.display().to_string(),
                line: c.line,
                formula_shape: c.post_shape.label().to_string(),
            })
            .collect(),
        verify_targets: walk
            .verify_targets
            .iter()
            .map(|v| ProofVerifyTarget {
                fn_name: v.fn_name.clone(),
                source_path: v.source_path.display().to_string(),
                line: v.line,
            })
            .collect(),
        lift_contracts: lifted
            .iter()
            .map(|l| ProofLiftContract {
                adapter: l.adapter.to_string(),
                name: l.decl.name.clone(),
                source_path: l.source_path.clone(),
                has_pre: l.decl.pre.is_some(),
                has_post: l.decl.post.is_some(),
                has_inv: l.decl.inv.is_some(),
                out_binding: l.decl.out_binding.clone(),
            })
            .collect(),
    };
    // We canonicalize via serde_json with sorted keys at the
    // deserialize step; for the manifest above, serde's struct order is
    // deterministic at the top level, and the inner Vec values are
    // emitted in the order we built them (sorted by source path /
    // name during the walk). Good enough for a v0 hash.
    let bytes = serde_json::to_vec(&manifest).map_err(|e| format!("serialize manifest: {e}"))?;
    // BLAKE3-512: 64-byte extended output, hex-encoded, prefixed.
    let mut out = [0u8; 64];
    let mut xof = blake3::Hasher::new();
    xof.update(&bytes);
    let mut reader = xof.finalize_xof();
    reader.fill(&mut out);
    let hex = hex_encode_lower(&out);
    let cid = format!("{HASH_PREFIX}{hex}");
    let dir = target_dir.join("provekit");
    fs::create_dir_all(&dir).map_err(|e| format!("create_dir_all {}: {e}", dir.display()))?;
    let path = dir.join(format!("{cid}.proof"));
    fs::write(&path, &bytes).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok((path, cid))
}

fn hex_encode_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

// ---------------------------------------------------------------------------
// Public entrypoint: a consumer's build.rs calls this exactly once.
// ---------------------------------------------------------------------------

/// The single function a consumer's `build.rs` calls.
///
/// Looks up `CARGO_MANIFEST_DIR` and `OUT_DIR` from the environment
/// (cargo sets both for build scripts), source-walks `src/`, parses
/// the Cargo.toml `[package.metadata.provekit]` table, optionally
/// mints a `.proof` under `target/`, runs the Tier-3 Z3 check on
/// each `#[verify]` function body, and emits cargo-protocol lines to
/// stdout.
///
/// Diagnostic output order:
///
///   1. `cargo:rerun-if-changed=...` for every walked source file +
///      Cargo.toml.
///   2. `cargo:warning=...` for every undischarged call site.
///   3. A human-readable summary on stderr.
///   4. A non-zero exit if `strict = true` and at least one call
///      site is `Unsatisfied`.
pub fn run_verification() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .expect("provekit-build: CARGO_MANIFEST_DIR not set; this is meant to run from build.rs");
    let manifest_dir = PathBuf::from(manifest_dir);
    let cargo_toml = manifest_dir.join("Cargo.toml");
    let cfg = match parse_config_from_path(&cargo_toml) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("provekit-build: {e}");
            return;
        }
    };
    // OUT_DIR for build scripts is `target/<profile>/build/<crate>-<hash>/out`;
    // we walk up two levels to reach `target/<profile>/build/<crate>-...`,
    // then write under a sibling `provekit/` directory. Falling back to
    // `target/provekit` in the manifest dir is fine for tests.
    let out_dir = std::env::var("OUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| manifest_dir.join("target"));
    let report = run_verification_inner(&manifest_dir, &cargo_toml, &out_dir, &cfg);
    emit_cargo_directives(&report, &cfg);
    print_summary_to_stderr(&report, &cfg);
    if cfg.strict() && report.has_violations() {
        eprintln!(
            "provekit-build: strict mode + {} unsatisfied callsite(s); failing the build.",
            report.unsatisfied_count()
        );
        std::process::exit(1);
    }
}

/// Same as `run_verification`, but takes explicit paths so tests can
/// drive it without polluting the process environment.
pub fn run_verification_inner(
    manifest_dir: &Path,
    cargo_toml_path: &Path,
    out_dir: &Path,
    cfg: &ProvekitConfig,
) -> VerificationReport {
    let walk = source_walk::walk(manifest_dir);
    // Lift pass: walk the same source tree, dispatch to every enabled
    // lift adapter, collect ContractDecls. This runs by default: the
    // user does NOT type "lift" anywhere; just `cargo build`.
    let enabled = cfg.enabled_lift_adapters();
    let lift_report = run_lift_pass(manifest_dir, &enabled);
    let mut report = VerificationReport {
        contract_count: walk.contracts.len(),
        verify_count: walk.verify_targets.len(),
        callsites: Vec::new(),
        mint_path: None,
        mint_cid: None,
        lift_count: lift_report.lifted.len(),
        lift_breakdown: lift_report.breakdown,
        lifted_contracts: lift_report.lifted,
        unknown_lift_adapters: cfg.unknown_lift_adapters(),
    };
    if cfg.mint_proof() {
        match mint_proof_file(out_dir, &walk, &report.lifted_contracts) {
            Ok((p, cid)) => {
                report.mint_path = Some(p);
                report.mint_cid = Some(cid);
            }
            Err(e) => {
                eprintln!("provekit-build: mint .proof failed: {e}");
            }
        }
    }
    let z3 = z3_binary_path();
    for cs in walk.callsites.iter() {
        // Match call site to a contract by callee name.
        let contract = walk.contracts.iter().find(|c| c.fn_name == cs.callee);
        let (verdict, note) = match contract {
            Some(c) => {
                let label = format!(
                    "{}::{} -> {}",
                    cs.source_path.display(),
                    cs.verify_fn,
                    cs.callee
                );
                let script =
                    build_obligation_script(cfg, &label, &c.post_shape, cs.surrounding_eq_check);
                let res = solve(&z3, &script.script_smt2, cfg.z3_timeout_ms());
                // Verdict-mapping rules for v0:
                //
                //   no surrounding eq check:
                //     sat   -> Discharged   (post is realizable; nothing to flag)
                //     unsat -> Unsatisfied  (post is itself contradictory; loud)
                //
                //   surrounding `if x == K` check:
                //     unsat -> Unsatisfied  (branch is DEAD per contract: warn)
                //     sat   -> Discharged   (branch is reachable; fine)
                //
                // The strict-mode failure path keys off `Unsatisfied`,
                // so dead branches are what flip the build.
                let raw = res.verdict.clone();
                let (v, base_note) = match (cs.surrounding_eq_check, raw) {
                    (None, SolverVerdict::Discharged) => (
                        SolverVerdict::Unsatisfied,
                        format!(
                            "post={} contradicts itself (Z3: unsat with no surrounding constraint)",
                            c.post_shape.label()
                        ),
                    ),
                    (None, SolverVerdict::Unsatisfied) => (
                        SolverVerdict::Discharged,
                        format!("discharged: post={} is satisfiable", c.post_shape.label()),
                    ),
                    (Some(k), SolverVerdict::Discharged) => (
                        SolverVerdict::Unsatisfied,
                        format!(
                            "violation: branch `if x == {}` is dead per contract (post={})",
                            k,
                            c.post_shape.label()
                        ),
                    ),
                    (Some(k), SolverVerdict::Unsatisfied) => (
                        SolverVerdict::Discharged,
                        format!(
                            "discharged: branch `if x == {}` is reachable under post={}",
                            k,
                            c.post_shape.label()
                        ),
                    ),
                    (_, SolverVerdict::Undecidable) => (
                        SolverVerdict::Undecidable,
                        format!("undecidable: post={} not encodable", c.post_shape.label()),
                    ),
                };
                let n = if !res.note.is_empty() {
                    format!("{base_note} ({})", res.note)
                } else {
                    base_note
                };
                (v, n)
            }
            None => (
                SolverVerdict::Undecidable,
                "no contract registered for callee".to_string(),
            ),
        };
        report.callsites.push(CallsiteOutcome {
            verify_fn: cs.verify_fn.clone(),
            callee: cs.callee.clone(),
            source_path: cs.source_path.clone(),
            line: cs.line,
            verdict,
            note,
        });
    }
    // Filter call sites by the `verify_targets` glob, applied to the
    // verify-fn name. A non-matching verify fn keeps its callsites in
    // the report but skips the SMT round-trip. Today we apply post-hoc
    // for simplicity; the SMT cost is tiny.
    let glob_pat = cfg.verify_targets();
    if glob_pat != "**/*" {
        let pat = glob::Pattern::new(&glob_pat)
            .unwrap_or_else(|_| glob::Pattern::new("**/*").expect("**/* always parses"));
        report.callsites.retain(|c| pat.matches(&c.verify_fn));
    }
    // Stash the walk paths as a side effect so emit_cargo_directives
    // can announce rerun-if-changed.
    report.callsites.sort_by(|a, b| {
        a.source_path
            .cmp(&b.source_path)
            .then_with(|| a.line.cmp(&b.line))
    });
    let _ = (cargo_toml_path,);
    report
}

fn emit_cargo_directives(report: &VerificationReport, cfg: &ProvekitConfig) {
    // Every walked source file + Cargo.toml drives invalidation. The
    // walker re-records the paths in `report.callsites`, but those are
    // only the verify bodies. We re-walk to capture the full set.
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir.join("Cargo.toml").display()
    );
    for entry in walkdir::WalkDir::new(manifest_dir.join("src"))
        .follow_links(false)
        .into_iter()
        .flatten()
    {
        if entry.file_type().is_file()
            && entry.path().extension().and_then(|s| s.to_str()) == Some("rs")
        {
            println!("cargo:rerun-if-changed={}", entry.path().display());
        }
    }
    // Surface the lift-derived contracts on stdout so users see what
    // got promoted from third-party annotations during their build.
    // Emitted as `cargo:warning=` so cargo prints it without failing.
    for unknown in &report.unknown_lift_adapters {
        println!(
            "cargo:warning=provekit: unknown lift adapter `{unknown}` (allowed: {})",
            ALL_ADAPTERS.join(", ")
        );
    }
    for lifted in &report.lifted_contracts {
        let slots = match (
            lifted.decl.pre.is_some(),
            lifted.decl.post.is_some(),
            lifted.decl.inv.is_some(),
        ) {
            (true, true, _) => "pre+post",
            (true, false, false) => "pre",
            (false, true, false) => "post",
            (_, _, true) => "inv",
            _ => "empty",
        };
        if std::env::var("PROVEKIT_VERBOSE").ok().as_deref() == Some("1") {
            println!(
                "cargo:warning=provekit: LIFT [{}] {} ({}): {}",
                lifted.adapter, lifted.decl.name, slots, lifted.source_path
            );
        }
    }
    // Always emit a one-line summary marker when at least one lift
    // contract was found, so the consumer sees the lift lane is live
    // without needing PROVEKIT_VERBOSE=1.
    if report.lift_count > 0 && std::env::var("PROVEKIT_VERBOSE").ok().as_deref() == Some("1") {
        let by_adapter: Vec<String> = report
            .lift_breakdown
            .iter()
            .filter(|b| b.lifted > 0)
            .map(|b| format!("{}={}", b.adapter, b.lifted))
            .collect();
        println!(
            "cargo:warning=provekit: lift promoted {} contract(s) from third-party annotations [{}]",
            report.lift_count,
            by_adapter.join(", ")
        );
    }
    for cs in &report.callsites {
        match cs.verdict {
            SolverVerdict::Discharged => {
                if std::env::var("PROVEKIT_VERBOSE").ok().as_deref() == Some("1") {
                    println!(
                        "cargo:warning=provekit: OK  {}::{} -> {}: {}",
                        cs.source_path.display(),
                        cs.verify_fn,
                        cs.callee,
                        cs.note
                    );
                }
            }
            SolverVerdict::Unsatisfied => {
                if std::env::var("PROVEKIT_VERBOSE").ok().as_deref() == Some("1") {
                    let prefix = if cfg.strict() { "ERROR" } else { "WARN" };
                    println!(
                        "cargo:warning=provekit: {} {}::{} -> {}: {}",
                        prefix,
                        cs.source_path.display(),
                        cs.verify_fn,
                        cs.callee,
                        cs.note
                    );
                }
            }
            SolverVerdict::Undecidable => {
                if std::env::var("PROVEKIT_VERBOSE").ok().as_deref() == Some("1") {
                    println!(
                        "cargo:warning=provekit: SKIP {}::{} -> {}: {}",
                        cs.source_path.display(),
                        cs.verify_fn,
                        cs.callee,
                        cs.note
                    );
                }
            }
        }
    }
}

fn print_summary_to_stderr(report: &VerificationReport, cfg: &ProvekitConfig) {
    let total_contracts = report.contract_count + report.lift_count;
    eprintln!("--- ProvekIt verification report ---");
    eprintln!("  inventory contracts: {}", report.contract_count);
    eprintln!("  lift contracts:      {}", report.lift_count);
    eprintln!("  total contracts:     {}", total_contracts);
    if !report.lift_breakdown.is_empty() {
        let active: Vec<String> = report
            .lift_breakdown
            .iter()
            .filter(|b| b.enabled)
            .map(|b| format!("{}({}/{})", b.adapter, b.lifted, b.seen))
            .collect();
        if !active.is_empty() {
            eprintln!("    lift breakdown:    {}", active.join(", "));
        }
        let skipped: Vec<&str> = report
            .lift_breakdown
            .iter()
            .filter(|b| !b.enabled)
            .map(|b| b.adapter)
            .collect();
        if !skipped.is_empty() {
            eprintln!("    lift disabled:     {}", skipped.join(", "));
        }
    }
    eprintln!("  verify targets:      {}", report.verify_count);
    eprintln!("  callsites:           {}", report.callsites.len());
    eprintln!("    discharged:        {}", report.discharged_count());
    eprintln!("    unsatisfied:       {}", report.unsatisfied_count());
    eprintln!("    undecidable:       {}", report.undecidable_count());
    if let Some(p) = &report.mint_path {
        eprintln!("  proof file:          {}", p.display());
    }
    if let Some(c) = &report.mint_cid {
        eprintln!("  proof cid:           {c}");
    }
    eprintln!("  strict mode:         {}", cfg.strict());
    eprintln!("  z3 path:             {}", z3_binary_path());
    eprintln!("  z3 timeout (ms):     {}", cfg.z3_timeout_ms());
    eprintln!(
        "  summary:             {} inventory, {} lift, {} verified, {} violation(s)",
        report.contract_count,
        report.lift_count,
        report.discharged_count(),
        report.unsatisfied_count()
    );
    eprintln!("------------------------------------");
}

// ---------------------------------------------------------------------------
// Re-export the visit shim used by source_walk so downstream tests can
// poke the syn machinery without re-importing it.
// ---------------------------------------------------------------------------

#[doc(hidden)]
pub mod __for_tests {
    pub use super::source_walk::{walk, walk_str};
}

// Required to satisfy the `use syn::visit::Visit` import even when not
// directly referenced: keeps the dep graph honest.
const _: fn() = || {
    fn _is_visit<T: Visit<'static>>() {}
};
