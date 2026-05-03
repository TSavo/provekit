// SPDX-License-Identifier: Apache-2.0
//
// provekit-cli — user-facing CLI binary.
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

mod cmd_agent;
mod cmd_ask;
mod cmd_dump;
mod cmd_fix;
mod cmd_hash;
mod cmd_implicate;
mod cmd_init;
mod cmd_lift;
mod cmd_mint;
mod cmd_must;
mod cmd_prove;
mod cmd_search;
mod cmd_verify_protocol;
mod cmd_version;
mod cmd_witness;
mod project_config;
mod prompts;
mod protocol;
mod report_fmt;

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

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Run the six-stage verifier: load proofs, enumerate callsites, solve obligations, report.
    Prove(ProveArgs),
    /// Same as `prove`. Reserved for a future split.
    Verify(ProveArgs),
    /// Look up a formula by content. Parses an IR-JSON formula file, hashes it, reports the CID.
    Ask(AskArgs),
    /// Search proof catalogs by content hash.
    Search(SearchArgs),
    /// Mint an implication memento (antecedent CID -> consequent CID) via Z3.
    Implicate(ImplicateArgs),
    /// Short alias for `implicate`.
    Imp(ImplicateArgs),
    /// Pretty-print a .proof envelope: members, bodies, signatures.
    Dump(DumpArgs),
    /// Compute the BLAKE3-512 self-identifying CID of a file (or stdin).
    Hash(HashArgs),
    /// Initialize a project: provekit.toml, .provekit/, sample invariant, GitHub Action.
    Init(InitArgs),
    /// Print directions for the lift adapter (TS only in v1.0; planned for v1.2.0).
    Lift(LiftArgs),
    /// Run the LLM-assisted lift loop on a source file.
    AgentLift(cmd_lift::AgentLiftArgs),
    /// Dispatch the lift-plugin protocol: spawn the configured plugin, write its `.proof`.
    Mint(cmd_mint::MintArgs),
    /// Translate an English description to a verified ProvekIt contract via the configured agent.
    Must(cmd_must::MustArgs),
    /// Hand the configured agent a bug; verify a fix in a sandbox; report.
    Fix(cmd_fix::FixArgs),
    /// Mint a witness: prove a new property from an existing contract,
    /// extending the proof lattice. Anyone can mint witnesses.
    Witness(WitnessArgs),
    /// List or describe installed agent plugins.
    Agent(cmd_agent::AgentArgs),
    /// Confirm the local install conforms to the expected protocol-catalog CID.
    VerifyProtocol(VerifyProtocolArgs),
    /// Print CLI version and the protocol catalog CID it declares conformance to.
    Version(VersionArgs),
}

#[derive(Parser, Debug, Clone)]
pub struct ProveArgs {
    /// Project root to scan for .proof files. Defaults to the current directory.
    pub project: Option<PathBuf>,
    /// Path to z3 binary (default: "z3" on PATH).
    #[arg(long, default_value = "z3")]
    pub z3: String,
    #[command(flatten)]
    pub out: OutputFlags,
}

#[derive(Parser, Debug, Clone)]
pub struct AskArgs {
    /// Path to an IR-JSON formula file. Use `-` for stdin.
    pub formula: PathBuf,
    /// Project root to search. Defaults to current directory.
    #[arg(long)]
    pub project: Option<PathBuf>,
    #[command(flatten)]
    pub out: OutputFlags,
}

#[derive(Parser, Debug, Clone)]
pub struct SearchArgs {
    /// Search formulas appearing in the consequent slot of implications.
    #[arg(long, conflicts_with = "antecedent")]
    pub consequent: Option<PathBuf>,
    /// Search formulas appearing in the antecedent slot of implications.
    #[arg(long, conflicts_with = "consequent")]
    pub antecedent: Option<PathBuf>,
    /// Project root to search. Defaults to current directory.
    #[arg(long)]
    pub project: Option<PathBuf>,
    #[command(flatten)]
    pub out: OutputFlags,
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
    /// Source file to lift (currently informational; the lift implementation lives in TS).
    pub file: Option<PathBuf>,
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
pub struct WitnessArgs {
    /// CID of the contract to witness from (the base theorem).
    pub contract_cid: String,
    /// Path to an IR-JSON formula file representing the property to prove.
    /// Use `-` for stdin.
    pub property: PathBuf,
    /// Project root to load the contract from. Defaults to current directory.
    #[arg(long)]
    pub project: Option<PathBuf>,
    /// Path to z3 binary (default: "z3" on PATH).
    #[arg(long, default_value = "z3")]
    pub z3: String,
    #[command(flatten)]
    pub out: OutputFlags,
}

#[derive(Parser, Debug, Clone)]
pub struct VersionArgs {
    #[command(flatten)]
    pub out: OutputFlags,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let code = match cli.cmd {
        Cmd::Prove(a) | Cmd::Verify(a) => cmd_prove::run(a),
        Cmd::Ask(a) => cmd_ask::run(a),
        Cmd::Search(a) => cmd_search::run(a),
        Cmd::Implicate(a) | Cmd::Imp(a) => cmd_implicate::run(a),
        Cmd::Dump(a) => cmd_dump::run(a),
        Cmd::Hash(a) => cmd_hash::run(a),
        Cmd::Init(a) => cmd_init::run(a),
        Cmd::Lift(a) => cmd_lift::run(a),
        Cmd::AgentLift(a) => cmd_lift::run_agent(a),
        Cmd::Mint(a) => cmd_mint::run(a),
        Cmd::Must(a) => cmd_must::run(a),
        Cmd::Fix(a) => cmd_fix::run(a),
        Cmd::Witness(a) => cmd_witness::run(a),
        Cmd::Agent(a) => cmd_agent::run(a),
        Cmd::VerifyProtocol(a) => cmd_verify_protocol::run(a),
        Cmd::Version(a) => cmd_version::run(a),
    };
    ExitCode::from(code)
}
