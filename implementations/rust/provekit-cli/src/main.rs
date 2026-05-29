// SPDX-License-Identifier: Apache-2.0
//
// provekit-cli: user-facing CLI binary.
//
// Pure routing crate: parses argv via clap, dispatches into the
// existing workspace crates (canonicalizer, proof-envelope,
// claim-envelope, ir-symbolic, verifier). Nothing crypto-load-bearing
// is implemented here; this is the seam between humans and libs.
//
// Exit codes:
//   0 success
//   1 verification failure
//   2 user error (bad args, file not found)
//   3 solver failure (z3 unavailable / timeout)

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod cmd_bind;
mod cmd_compose;
mod cmd_dump;
mod cmd_emit;
mod cmd_hash;
mod cmd_implicate;
mod cmd_init;
mod cmd_lift;
mod cmd_link;
mod cmd_materialize;
mod cmd_mint;
mod cmd_package;
mod cmd_plugin;
mod cmd_protocol;
mod cmd_prove;
mod cmd_recognize;
mod cmd_verify;
mod cmd_verify_protocol;
mod cmd_version;
mod kit_dispatch;
mod lift_plugin;
mod project_config;
mod protocol;
mod report_fmt;
mod sort_morphism_catalog;

/// Exit codes used across subcommands.
pub const EXIT_OK: u8 = 0;
pub const EXIT_VERIFY_FAIL: u8 = 1;
pub const EXIT_USER_ERROR: u8 = 2;
pub const EXIT_SOLVER_FAIL: u8 = 3;

#[derive(Parser, Debug)]
#[command(
    name = "provekit",
    version,
    about = "ProvekIt CLI: per-codebase invariant enforcement at git-commit speed.",
    long_about = "ProvekIt CLI. Walk .proof catalogs, verify property mementos, \
mint implications, hash content. Each subcommand wraps an existing \
ProvekIt Rust library; the CLI is routing only."
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

/// Common output flags. Each subcommand embeds these so users can pass
/// `--json` / `--quiet` after the subcommand name.
#[derive(Parser, Debug, Clone, Default)]
pub struct OutputFlags {
    /// Emit structured JSON instead of human-readable text.
    #[arg(long, global = true)]
    pub json: bool,
    /// Suppress non-error output.
    #[arg(long, global = true)]
    pub quiet: bool,
}

/// PEP 1.7.0 plugin flags.  Embed in any subcommand that participates in the
/// plugin registry (§7 / §9).  The registry seals once per run; every output's
/// provenance MUST cite the registry CID (§9.4).
///
/// Flag catalogue (§7):
///   --plugin <kind>:<source>     canonical form
///   --sugar <source>             alias for --plugin sugar:<source>
///   --loss-fn <source>           alias for --plugin loss-function:<source>
///   --lifter <source>            alias for --plugin lift:<source>  (wire kind = "lift")
///   --no-default-plugins         suppress ALL built-in plugin registration
///   --no-default-plugin <kind>   suppress built-ins for one kind
///   --strict-plugins             promote every plugin load failure to a refuse
///   --plugin-registry-out <path> write PluginRegistryMemento to <path> after sealing
pub use cmd_plugin::PluginFlags;

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Run the six-stage verifier: load proofs, enumerate callsites, solve obligations, report.
    Prove(ProveArgs),
    /// Work with protocol catalog evolution artifacts.
    Protocol(cmd_protocol::ProtocolArgs),
    /// Inspect package artifacts and supply-chain receipt inputs.
    Package(cmd_package::PackageArgs),
    /// Verify a kit end-to-end: lift its contract claims, discharge each
    /// via the solver-dispatch table, mint a signed witness citing the
    /// discharging solver, and emit a per-claim verification receipt.
    /// This is the real GATE verb (#1405); distinct from `prove` (the
    /// six-stage prover / `--kit` lift-plugin conformance gate).
    Verify(cmd_verify::VerifyArgs),
    /// Mint an implication memento (antecedent CID -> consequent CID) via Z3.
    Implicate(ImplicateArgs),
    /// Short alias for `implicate`.
    Imp(ImplicateArgs),
    /// Pretty-print a .proof envelope: members, bodies, signatures.
    Dump(DumpArgs),
    /// Compute the BLAKE3-512 self-identifying CID of a file (or stdin).
    Hash(HashArgs),
    /// Recognizer (per protocol §4.2.5): scan source for shapes that
    /// match published sugar binding templates; emit tags. The reverse
    /// direction of `materialize` — same kit, same AST machinery, two
    /// directions over one .proof envelope. Tron-named for the kit-side
    /// fingerprint scanner.
    Recognize(cmd_recognize::RecognizeArgs),
    /// Initialize a project: provekit.toml, .provekit/, sample invariant, GitHub Action.
    Init(InitArgs),
    /// Print directions for the lift adapter (TS only in v1.0; planned for v1.2.0).
    Lift(LiftArgs),
    /// Dispatch the lift-plugin protocol: spawn the configured plugin, write its `.proof`.
    Mint(cmd_mint::MintArgs),
    /// Emit target/framework test artifacts from neutral contract predicates.
    Emit(cmd_emit::EmitArgs),
    /// Confirm the local install conforms to the expected protocol-catalog CID.
    VerifyProtocol(VerifyProtocolArgs),
    /// Print CLI version and the protocol catalog CID it declares conformance to.
    Version(VersionArgs),
    /// Linker pass: derive bridges from (contracts ∪ call-edges), emit LinkBundle.
    /// Per spec protocol/specs/2026-05-03-bridge-linkage-protocol.md R2-R5.
    Link(LinkArgs),
    /// JSON-RPC subprocess transport for the canonical compose primitive.
    /// Per spec protocol/specs/2026-05-09-contract-composition-protocol.md §6.3.
    /// Reads JSON-RPC requests on stdin, writes responses on stdout.
    Compose(ComposeArgs),
    /// Bind concept contracts to source code: lift, cluster, name, scope, identify, realize, witness.
    /// Implements the eight-verb pipeline (paper 20 §9) against arbitrary user code.
    /// --rewrite={annotate,canonical,invisible} --mode={witness,emitter,monitor,gate} --target-language=<lang>
    Bind(cmd_bind::BindArgs),
    /// Materialize concept-citation carriers into library-bound source via substrate realize kits.
    Materialize(cmd_materialize::MaterializeArgs),
}

#[derive(Parser, Debug, Clone)]
pub struct ProveArgs {
    /// Project root to scan for .proof files. Defaults to the current directory.
    pub project: Option<PathBuf>,
    /// Path to z3 binary (default: "z3" on PATH).
    #[arg(long, default_value = "z3")]
    pub z3: String,
    /// Artifact bytes to verify against a package release proof/receipt.
    #[arg(long, requires = "proof")]
    pub artifact: Option<PathBuf>,
    /// Package release proof/receipt naming the expected binaryCid.
    #[arg(long)]
    pub proof: Option<PathBuf>,
    /// Consumer policy proof/receipt used for policy admission checks.
    #[arg(long, requires = "proof")]
    pub policy: Option<PathBuf>,
    /// Require that a concept has reached the empirically-witnessed promotion tier.
    #[arg(long = "require-empirically-witnessed")]
    pub require_empirically_witnessed: Option<String>,
    /// Fixture-state CID for tier queries such as --require-empirically-witnessed.
    #[arg(long = "require-fixture")]
    pub require_fixture: Option<String>,
    /// Consensus policy JSON used to evaluate a required empirical witness vector.
    #[arg(long = "consensus-policy", requires = "require_empirically_witnessed")]
    pub consensus_policy: Option<PathBuf>,
    /// Kit conformance gate: verify the named kit's lifter implements the
    /// canonical lift-plugin-protocol contracts (C1-C8). When set, the six-stage
    /// verifier is bypassed; instead the kit's lifter is spawned via JSON-RPC and
    /// each verifier in `provekit-self-contracts::lift_plugin_protocol` is run
    /// against the captured RPC messages.
    /// Known kits: rust, go, cpp, ts, csharp, clr-bytecode, evm-bytecode, swift, java, python, ruby, zig, c, php.
    #[arg(long, conflicts_with = "project")]
    pub kit: Option<String>,
    #[command(flatten)]
    pub out: OutputFlags,
    /// Additional project directories whose .proof files should also be loaded
    /// (e.g., an OpenAPI spec project for cross-kit verification).
    #[arg(long = "with", num_args = 0..)]
    pub with: Vec<String>,
}

#[derive(Parser, Debug, Clone)]
pub struct ImplicateArgs {
    /// Antecedent memento CID (full self-identifying string).
    pub antecedent: String,
    /// Consequent memento CID (full self-identifying string).
    pub consequent: String,
    /// Path to z3 binary (default: "z3" on PATH).
    #[arg(long, default_value = "z3")]
    pub z3: String,
    #[command(flatten)]
    pub out: OutputFlags,
}

#[derive(Parser, Debug, Clone)]
pub struct DumpArgs {
    /// Path to a .proof file.
    pub proof_file: PathBuf,
    #[command(flatten)]
    pub out: OutputFlags,
}

#[derive(Parser, Debug, Clone)]
pub struct HashArgs {
    /// File to hash. Reads stdin when omitted.
    pub file: Option<PathBuf>,
    #[command(flatten)]
    pub out: OutputFlags,
}

#[derive(Parser, Debug, Clone)]
pub struct InitArgs {
    /// Project directory to initialize. Defaults to current directory.
    pub project: Option<PathBuf>,
    /// Overwrite any pre-existing files.
    #[arg(long)]
    pub force: bool,
    #[command(flatten)]
    pub out: OutputFlags,
}

#[derive(Parser, Debug, Clone)]
pub struct LiftArgs {
    /// Project root to lift. Defaults to the current directory.
    pub project: Option<PathBuf>,
    /// Output file. Writes stdout when omitted or `-`.
    #[arg(short = 'o', long = "output")]
    pub output: Option<PathBuf>,
    /// Ask the configured lifter to report native contract identities without full ProofIR lowering.
    #[arg(long, conflicts_with = "library_bindings")]
    pub identify_only: bool,
    /// Ask the configured lifter for proof-producing host-language library-sugar bindings.
    #[arg(long, conflicts_with = "identify_only")]
    pub library_bindings: bool,
    #[command(flatten)]
    pub out: OutputFlags,
}

#[derive(Parser, Debug, Clone)]
pub struct VerifyProtocolArgs {
    /// Override the expected catalog CID.
    #[arg(long)]
    pub catalog: Option<String>,
    /// Also verify the signed catalog attestation (Ed25519 signature
    /// over the catalog's CID by the ProvekIt Foundation Root Key).
    /// Default uses the embedded `foundation-v0.pub` and embedded
    /// `catalog-signature-v1.4.0.json`; override via `--pubkey-file`
    /// and `--signature-file`.
    #[arg(long)]
    pub signed: bool,
    /// Override the public key file used to verify the signature.
    /// Format: `ed25519:<base64>`, possibly with surrounding whitespace.
    #[arg(long, requires = "signed")]
    pub pubkey_file: Option<PathBuf>,
    /// Override the signed attestation file.
    #[arg(long, requires = "signed")]
    pub signature_file: Option<PathBuf>,
    #[command(flatten)]
    pub out: OutputFlags,
}

#[derive(Parser, Debug, Clone)]
pub struct VersionArgs {
    #[command(subcommand)]
    pub cmd: Option<cmd_version::VersionCmd>,
    #[command(flatten)]
    pub out: OutputFlags,
}

#[derive(Parser, Debug, Clone)]
pub struct ComposeArgs {
    /// Speak JSON-RPC over stdin / stdout. Required today; the only
    /// transport the CCP §6.3 binding mode defines.
    #[arg(long)]
    pub rpc: bool,
    #[command(flatten)]
    pub out: OutputFlags,
}

#[derive(Parser, Debug, Clone)]
pub struct LinkArgs {
    /// Project root. Must contain rust-callee/ and go-caller/ subdirs.
    /// Defaults to current directory.
    pub project: Option<PathBuf>,
    /// Path to the go lsp binary (default: searches PATH for provekit-lsp-go,
    /// then falls back to `go run <project-root>/go-caller/`).
    #[arg(long)]
    pub go_lsp_bin: Option<String>,
    #[command(flatten)]
    pub out: OutputFlags,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let code = match cli.cmd {
        Cmd::Prove(a) => cmd_prove::run(a),
        Cmd::Verify(a) => cmd_verify::run(a),
        Cmd::Protocol(a) => cmd_protocol::run(a),
        Cmd::Package(a) => cmd_package::run(a),
        Cmd::Implicate(a) | Cmd::Imp(a) => cmd_implicate::run(a),
        Cmd::Dump(a) => cmd_dump::run(a),
        Cmd::Hash(a) => cmd_hash::run(a),
        Cmd::Recognize(a) => cmd_recognize::run(a),
        Cmd::Init(a) => cmd_init::run(a),
        Cmd::Lift(a) => cmd_lift::run(a),
        Cmd::Mint(a) => cmd_mint::run(a),
        Cmd::Emit(a) => cmd_emit::run(a),
        Cmd::VerifyProtocol(a) => cmd_verify_protocol::run(a),
        Cmd::Version(a) => cmd_version::run(a),
        Cmd::Link(a) => cmd_link::run(a),
        Cmd::Compose(a) => cmd_compose::run(a),
        Cmd::Bind(a) => cmd_bind::run(a),
        Cmd::Materialize(a) => cmd_materialize::run(a),
    };
    ExitCode::from(code)
}
