// SPDX-License-Identifier: Apache-2.0
//
// `provekit proof ...`: Rust CLI proof-artifact workflow.
//
// Other kits expose RPC surfaces and libraries. The Rust CLI is the
// operator surface that drives them and checks the universal `.proof`
// artifact they produce.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::{Parser, Subcommand};
use owo_colors::OwoColorize;
use provekit_canonicalizer::{blake3_512_of, Value as CValue};
use provekit_claim_envelope::{mint_contract, Authoring, MintContractArgs};
use provekit_proof_envelope::{
    build_proof_envelope, ed25519_pubkey_string, Ed25519Seed, ProofEnvelopeInput,
};
use provekit_verifier::cbor_decode::{decode, CborValue};
use provekit_verifier::proof_conformance::{
    validate_proof_file, ProofFileConformanceReport, ProofFormatConformanceWitness,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as Json};

use crate::OutputFlags;

const DEFAULT_GRAMMAR_CID: &str = "builtin:proof-file-format@2026-04-30";
const DEFAULT_INVARIANT_SET_CID: &str = "builtin:proof-file-format-invariants@0.1";
const DEFAULT_VERIFIER_CID: &str = "builtin:rust-cli-proof-check@0.1";
const PROOF_PROTOCOL_FIXTURES_METADATA_KEY: &str = "provekit.proofProtocol.fixtures.v0";

#[derive(Parser, Debug, Clone)]
pub struct ProofArgs {
    #[command(subcommand)]
    pub cmd: ProofCmd,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ProofCmd {
    /// Hash a .proof file as blake3-512:<hex>.
    Hash(ProofHashArgs),
    /// Inspect a .proof file without producing a conformance witness.
    Inspect(ProofInspectArgs),
    /// Check .proof conformance and emit a witness-shaped result.
    Check(ProofCheckArgs),
    /// Witness that a program implements the .proof protocol consumer surface.
    Implements(ProofImplementsArgs),
    /// Mint the canonical .proof protocol corpus and fixture manifest.
    MintProtocol(ProofMintProtocolArgs),
}

#[derive(Parser, Debug, Clone)]
pub struct ProofHashArgs {
    pub proof_file: PathBuf,
    #[command(flatten)]
    pub out: OutputFlags,
}

#[derive(Parser, Debug, Clone)]
pub struct ProofInspectArgs {
    pub proof_file: PathBuf,
    #[command(flatten)]
    pub out: OutputFlags,
}

#[derive(Parser, Debug, Clone)]
pub struct ProofCheckArgs {
    pub proof_file: PathBuf,
    /// The .proof artifact carrying the proof-file-format protocol surface.
    #[arg(long)]
    pub proof_format_proof: Option<PathBuf>,
    /// Policy CID or policy name governing the conformance check.
    #[arg(long)]
    pub policy: String,
    /// Grammar artifact CID. Defaults to the built-in .proof grammar identity.
    #[arg(long, default_value = DEFAULT_GRAMMAR_CID)]
    pub grammar: String,
    /// Invariant-set CID. Defaults to the built-in .proof invariant identity.
    #[arg(long, default_value = DEFAULT_INVARIANT_SET_CID)]
    pub invariants: String,
    /// Verifier identity CID. Defaults to this Rust CLI checker identity.
    #[arg(long, default_value = DEFAULT_VERIFIER_CID)]
    pub verifier: String,
    #[command(flatten)]
    pub out: OutputFlags,
}

#[derive(Parser, Debug, Clone)]
pub struct ProofImplementsArgs {
    /// Program artifact whose .proof protocol consumer behavior is being witnessed.
    #[arg(long)]
    pub program: PathBuf,
    /// The .proof artifact carrying the proof protocol surface and fixture seed.
    #[arg(long)]
    pub proof_protocol: PathBuf,
    /// Policy CID or policy name governing the implementation witness.
    #[arg(long)]
    pub policy: String,
    /// Grammar artifact CID passed through to the program's proof checker.
    #[arg(long, default_value = DEFAULT_GRAMMAR_CID)]
    pub grammar: String,
    /// Invariant-set CID passed through to the program's proof checker.
    #[arg(long, default_value = DEFAULT_INVARIANT_SET_CID)]
    pub invariants: String,
    /// Verifier identity expected for the program's proof checker.
    #[arg(long, default_value = DEFAULT_VERIFIER_CID)]
    pub verifier: String,
    #[command(flatten)]
    pub out: OutputFlags,
}

#[derive(Parser, Debug, Clone)]
pub struct ProofMintProtocolArgs {
    /// Directory where the proof protocol artifact and fixtures are written.
    #[arg(long)]
    pub out_dir: PathBuf,
    #[command(flatten)]
    pub out: OutputFlags,
}

#[derive(Debug, Serialize)]
struct ProofProtocolImplementationWitness {
    kind: String,
    schema_version: String,
    claim_kind: String,
    result: bool,
    program_cid: String,
    program_path: String,
    proof_protocol_cid: String,
    proof_protocol_path: String,
    policy_cid: String,
    consumer_model: String,
    fixture_count: usize,
    checks: Vec<Json>,
    errors: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ProofProtocolCorpus {
    kind: String,
    schema_version: String,
    protocol_cid: String,
    protocol_path: String,
    fixture_count: usize,
    fixtures: Vec<ProofProtocolCorpusFixture>,
}

#[derive(Debug, Serialize)]
struct ProofProtocolCorpusFixture {
    name: String,
    cid: String,
    path: String,
    expected: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct ProofProtocolFixtureManifest {
    fixtures: Vec<ProofProtocolFixture>,
}

#[derive(Debug, Clone, Deserialize)]
struct ProofProtocolFixture {
    name: String,
    cid: String,
    path: String,
    expected: bool,
}

pub fn run(args: ProofArgs) -> u8 {
    match args.cmd {
        ProofCmd::Hash(args) => run_hash(args),
        ProofCmd::Inspect(args) => run_inspect(args),
        ProofCmd::Check(args) => run_check(args),
        ProofCmd::Implements(args) => run_implements(args),
        ProofCmd::MintProtocol(args) => run_mint_protocol(args),
    }
}

fn run_hash(args: ProofHashArgs) -> u8 {
    match std::fs::read(&args.proof_file) {
        Ok(bytes) => {
            let cid = blake3_512_of(&bytes);
            if args.out.json {
                println!("{}", json!({"cid": cid, "bytes": bytes.len()}));
            } else if !args.out.quiet {
                println!("{cid}");
            }
            crate::EXIT_OK
        }
        Err(e) => {
            eprintln!(
                "{}: read {}: {e}",
                "error".red().bold(),
                args.proof_file.display()
            );
            crate::EXIT_USER_ERROR
        }
    }
}

fn run_inspect(args: ProofInspectArgs) -> u8 {
    let report = validate_proof_file(&args.proof_file);
    if args.out.json {
        match serde_json::to_string_pretty(&report) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("{}: serialize report: {e}", "error".red().bold());
                return crate::EXIT_USER_ERROR;
            }
        }
    } else if !args.out.quiet {
        print_report(&report);
    }
    if report.ok() {
        crate::EXIT_OK
    } else {
        crate::EXIT_VERIFY_FAIL
    }
}

fn run_check(args: ProofCheckArgs) -> u8 {
    let format_proof_cid = match args.proof_format_proof.as_ref() {
        Some(path) => {
            let format_report = validate_proof_file(path);
            if !format_report.ok() {
                if args.out.json {
                    let payload = json!({
                        "kind": "ProofFormatConformanceWitness",
                        "schema_version": "0.1",
                        "claim_kind": "proof-format-conformance",
                        "result": false,
                        "format_proof_cid": format_report.file_cid,
                        "policy_cid": args.policy,
                        "error": "proof-format proof is not itself a valid .proof artifact",
                        "format_report": format_report,
                    });
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&payload)
                            .unwrap_or_else(|_| payload.to_string())
                    );
                } else {
                    eprintln!(
                        "{}: proof-format proof {} is not conformant",
                        "error".red().bold(),
                        path.display()
                    );
                    for error in &format_report.errors {
                        eprintln!("  - {}: {}", error.rule_id, error.message);
                    }
                }
                return crate::EXIT_VERIFY_FAIL;
            }
            Some(format_report.file_cid)
        }
        None => None,
    };

    let report = validate_proof_file(&args.proof_file);
    let mut witness = ProofFormatConformanceWitness::from_report(
        report,
        args.grammar,
        args.invariants,
        args.verifier,
        args.policy,
    );
    if let Some(format_proof_cid) = format_proof_cid {
        witness = witness.with_format_proof_cid(format_proof_cid);
    }
    if args.out.json {
        match serde_json::to_string_pretty(&witness) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("{}: serialize witness: {e}", "error".red().bold());
                return crate::EXIT_USER_ERROR;
            }
        }
    } else if !args.out.quiet {
        println!("{}", "ProvekIt .proof conformance".bold());
        println!("  subject CID : {}", witness.subject_cid.cyan());
        if let Some(format_proof_cid) = &witness.format_proof_cid {
            println!("  format proof: {}", format_proof_cid.cyan());
        }
        let result = if witness.result {
            "true".green().to_string()
        } else {
            "false".red().to_string()
        };
        println!("  result      : {result}");
        println!("  policy      : {}", witness.policy_cid);
        println!("  grammar     : {}", witness.grammar_cid);
        println!("  invariants  : {}", witness.invariant_set_cid);
        println!("  verifier    : {}", witness.verifier_cid);
        println!("  members     : {}", witness.report.member_count);
        if !witness.report.warnings.is_empty() {
            println!();
            println!("  warnings:");
            for warning in &witness.report.warnings {
                println!("    - {warning}");
            }
        }
        if !witness.report.errors.is_empty() {
            println!();
            println!("  errors:");
            for error in &witness.report.errors {
                println!("    - {}: {}", error.rule_id, error.message);
            }
        }
    }
    if witness.result {
        crate::EXIT_OK
    } else {
        crate::EXIT_VERIFY_FAIL
    }
}

fn run_implements(args: ProofImplementsArgs) -> u8 {
    let program_bytes = match std::fs::read(&args.program) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!(
                "{}: read program {}: {e}",
                "error".red().bold(),
                args.program.display()
            );
            return crate::EXIT_USER_ERROR;
        }
    };
    let program_cid = blake3_512_of(&program_bytes);

    let protocol_report = validate_proof_file(&args.proof_protocol);
    let proof_protocol_cid = protocol_report.file_cid.clone();
    let mut errors = Vec::new();
    if !protocol_report.ok() {
        errors.push("proof protocol artifact is not a valid .proof file".to_string());
        for error in &protocol_report.errors {
            errors.push(format!("{}: {}", error.rule_id, error.message));
        }
    }

    let fixtures = if protocol_report.ok() {
        match load_protocol_fixture_manifest(&args.proof_protocol) {
            Ok(fixtures) => fixtures,
            Err(e) => {
                errors.push(e);
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    let mut checks = Vec::new();
    if protocol_report.ok() {
        for fixture in &fixtures {
            match resolve_fixture_path(&args.proof_protocol, fixture)
                .and_then(|path| verify_fixture_cid(&path, fixture).map(|_| path))
                .and_then(|path| run_program_proof_check(&args, &path))
            {
                Ok(check) => {
                    let got = check.get("result").and_then(|v| v.as_bool());
                    if got != Some(fixture.expected) {
                        errors.push(format!(
                            "fixture `{}` expected result {}, got {:?}",
                            fixture.name, fixture.expected, got
                        ));
                    }
                    checks.push(check);
                }
                Err(e) => errors.push(format!("fixture `{}`: {e}", fixture.name)),
            }
        }
    }

    let witness = ProofProtocolImplementationWitness {
        kind: "ProofProtocolImplementationWitness".into(),
        schema_version: "0.1".into(),
        claim_kind: "proof-protocol-implementation".into(),
        result: errors.is_empty(),
        program_cid,
        program_path: args.program.display().to_string(),
        proof_protocol_cid,
        proof_protocol_path: args.proof_protocol.display().to_string(),
        policy_cid: args.policy,
        consumer_model: "cli-executable".into(),
        fixture_count: fixtures.len(),
        checks,
        errors,
    };

    if args.out.json {
        match serde_json::to_string_pretty(&witness) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!(
                    "{}: serialize implementation witness: {e}",
                    "error".red().bold()
                );
                return crate::EXIT_USER_ERROR;
            }
        }
    } else if !args.out.quiet {
        println!("{}", "ProvekIt .proof protocol implementation".bold());
        println!("  program     : {}", witness.program_path);
        println!("  program CID : {}", witness.program_cid.cyan());
        println!("  protocol CID: {}", witness.proof_protocol_cid.cyan());
        println!("  policy      : {}", witness.policy_cid);
        let result = if witness.result {
            "true".green().to_string()
        } else {
            "false".red().to_string()
        };
        println!("  result      : {result}");
        println!("  fixtures    : {}", witness.fixture_count);
        println!("  checks      : {}", witness.checks.len());
        if !witness.errors.is_empty() {
            println!();
            println!("  errors:");
            for error in &witness.errors {
                println!("    - {error}");
            }
        }
    }

    if witness.result {
        crate::EXIT_OK
    } else {
        crate::EXIT_VERIFY_FAIL
    }
}

fn run_mint_protocol(args: ProofMintProtocolArgs) -> u8 {
    match mint_protocol_corpus(&args.out_dir) {
        Ok(corpus) => {
            if args.out.json {
                match serde_json::to_string_pretty(&corpus) {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        eprintln!("{}: serialize corpus: {e}", "error".red().bold());
                        return crate::EXIT_USER_ERROR;
                    }
                }
            } else if !args.out.quiet {
                println!("{}", "ProvekIt proof protocol corpus".bold());
                println!("  protocol CID : {}", corpus.protocol_cid.cyan());
                println!("  protocol path: {}", corpus.protocol_path);
                println!("  fixtures     : {}", corpus.fixture_count);
                for fixture in &corpus.fixtures {
                    println!(
                        "    - {} [{}] {}",
                        fixture.name,
                        if fixture.expected { "valid" } else { "invalid" },
                        fixture.path
                    );
                }
            }
            crate::EXIT_OK
        }
        Err(e) => {
            eprintln!("{}: {e}", "error".red().bold());
            crate::EXIT_USER_ERROR
        }
    }
}

fn mint_protocol_corpus(out_dir: &Path) -> Result<ProofProtocolCorpus, String> {
    std::fs::create_dir_all(out_dir).map_err(|e| format!("create {}: {e}", out_dir.display()))?;

    let valid = build_fixture_proof("@provekit/proof-format/fixture/valid-basic", 0x71);
    let valid_path = write_cid_named_proof(out_dir, &valid.cid, &valid.bytes)?;
    let valid_name = file_name_string(&valid_path)?;

    let invalid = build_fixture_proof("@provekit/proof-format/fixture/invalid-filename-cid", 0x72);
    let invalid_name = "invalid-filename-cid.proof";
    let invalid_path = out_dir.join(invalid_name);
    std::fs::write(&invalid_path, &invalid.bytes)
        .map_err(|e| format!("write {}: {e}", invalid_path.display()))?;

    let fixtures = vec![
        ProofProtocolCorpusFixture {
            name: "valid-basic-proof".into(),
            cid: valid.cid.clone(),
            path: valid_name,
            expected: true,
        },
        ProofProtocolCorpusFixture {
            name: "invalid-filename-cid".into(),
            cid: invalid.cid.clone(),
            path: invalid_name.into(),
            expected: false,
        },
    ];

    let manifest = serde_json::to_string(&json!({ "fixtures": fixtures }))
        .map_err(|e| format!("serialize fixture manifest: {e}"))?;
    let mut metadata = BTreeMap::new();
    metadata.insert(PROOF_PROTOCOL_FIXTURES_METADATA_KEY.into(), manifest);
    metadata.insert(
        "provekit.proofProtocol.grammarCid".into(),
        DEFAULT_GRAMMAR_CID.into(),
    );
    metadata.insert(
        "provekit.proofProtocol.invariantSetCid".into(),
        DEFAULT_INVARIANT_SET_CID.into(),
    );
    metadata.insert(
        "provekit.proofProtocol.verifierCid".into(),
        DEFAULT_VERIFIER_CID.into(),
    );
    metadata.insert(
        "provekit.proofProtocol.policyCid".into(),
        "builtin:proof-format-v0".into(),
    );

    let protocol = build_protocol_proof(metadata);
    let protocol_path = write_cid_named_proof(out_dir, &protocol.cid, &protocol.bytes)?;
    let cid_path = out_dir.join("proof-protocol.cid.txt");
    std::fs::write(&cid_path, format!("{}\n", protocol.cid))
        .map_err(|e| format!("write {}: {e}", cid_path.display()))?;

    Ok(ProofProtocolCorpus {
        kind: "ProofProtocolCorpus".into(),
        schema_version: "0.1".into(),
        protocol_cid: protocol.cid,
        protocol_path: protocol_path.display().to_string(),
        fixture_count: fixtures.len(),
        fixtures,
    })
}

struct BuiltProof {
    cid: String,
    bytes: Vec<u8>,
}

fn build_protocol_proof(metadata: BTreeMap<String, String>) -> BuiltProof {
    let signer_seed: Ed25519Seed = [0x70; 32];
    let signer_pubkey = ed25519_pubkey_string(&signer_seed);
    let input = ProofEnvelopeInput {
        name: "@provekit/proof-format-protocol".into(),
        version: "0.1.0".into(),
        binary_cid: None,
        metadata: Some(metadata),
        members: BTreeMap::new(),
        signer_cid: signer_pubkey,
        signer_seed,
        declared_at: "2026-05-06T00:00:00.000Z".into(),
    };
    let built = build_proof_envelope(&input);
    BuiltProof {
        cid: built.cid,
        bytes: built.bytes,
    }
}

fn build_fixture_proof(name: &str, seed_byte: u8) -> BuiltProof {
    let signer_seed: Ed25519Seed = [seed_byte; 32];
    let declared_at = "2026-05-06T00:00:00.000Z";
    let member = mint_contract(&MintContractArgs {
        formals: Vec::new(),
        emit_empty_formals: false,
        formal_sorts: Vec::new(),
        contract_name: format!("{name}/contract"),
        pre: Some(CValue::boolean(true)),
        post: None,
        inv: None,
        out_binding: "out".into(),
        produced_by: "provekit-proof-protocol-corpus@0.1".into(),
        produced_at: declared_at.into(),
        input_cids: vec![],
        authoring: Authoring::KitAuthor {
            author: "provekit-proof-protocol-corpus@0.1".into(),
            note: Some("canonical proof protocol fixture".into()),
        },
        signer_seed,
    })
    .expect("static fixture contract should mint");
    let mut members = BTreeMap::new();
    members.insert(member.cid, member.canonical_bytes);
    let signer_pubkey = ed25519_pubkey_string(&signer_seed);
    let input = ProofEnvelopeInput {
        name: name.into(),
        version: "0.1.0".into(),
        binary_cid: None,
        metadata: None,
        members,
        signer_cid: signer_pubkey,
        signer_seed,
        declared_at: declared_at.into(),
    };
    let built = build_proof_envelope(&input);
    BuiltProof {
        cid: built.cid,
        bytes: built.bytes,
    }
}

fn write_cid_named_proof(out_dir: &Path, cid: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    let path = out_dir.join(format!("{cid}.proof"));
    std::fs::write(&path, bytes).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(path)
}

fn file_name_string(path: &Path) -> Result<String, String> {
    path.file_name()
        .and_then(|s| s.to_str())
        .map(str::to_string)
        .ok_or_else(|| format!("path {} has no UTF-8 file name", path.display()))
}

fn load_protocol_fixture_manifest(
    protocol_path: &std::path::Path,
) -> Result<Vec<ProofProtocolFixture>, String> {
    let bytes = std::fs::read(protocol_path)
        .map_err(|e| format!("read proof protocol {}: {e}", protocol_path.display()))?;
    let catalog = decode(&bytes)
        .map_err(|e| format!("decode proof protocol {}: {e}", protocol_path.display()))?;
    let root = catalog
        .as_map()
        .ok_or_else(|| "proof protocol root is not a CBOR map".to_string())?;
    let metadata = root
        .get("metadata")
        .and_then(CborValue::as_map)
        .ok_or_else(|| {
            format!(
                "proof protocol metadata missing `{PROOF_PROTOCOL_FIXTURES_METADATA_KEY}` fixture manifest"
            )
        })?;
    let manifest_text = metadata
        .get(PROOF_PROTOCOL_FIXTURES_METADATA_KEY)
        .and_then(CborValue::as_tstr)
        .ok_or_else(|| {
            format!(
                "proof protocol metadata key `{PROOF_PROTOCOL_FIXTURES_METADATA_KEY}` missing or not text"
            )
        })?;
    let manifest: ProofProtocolFixtureManifest = serde_json::from_str(manifest_text)
        .map_err(|e| format!("parse fixture manifest JSON: {e}"))?;
    if manifest.fixtures.is_empty() {
        return Err("proof protocol fixture manifest is empty".to_string());
    }
    Ok(manifest.fixtures)
}

fn resolve_fixture_path(
    protocol_path: &std::path::Path,
    fixture: &ProofProtocolFixture,
) -> Result<PathBuf, String> {
    let fixture_path = PathBuf::from(&fixture.path);
    if fixture_path.is_absolute() {
        return Ok(fixture_path);
    }
    let base = protocol_path.parent().ok_or_else(|| {
        format!(
            "proof protocol path {} has no parent",
            protocol_path.display()
        )
    })?;
    Ok(base.join(fixture_path))
}

fn verify_fixture_cid(
    path: &std::path::Path,
    fixture: &ProofProtocolFixture,
) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read fixture {}: {e}", path.display()))?;
    let got = blake3_512_of(&bytes);
    if got != fixture.cid {
        return Err(format!(
            "fixture CID mismatch for {}: got {got}, expected {}",
            path.display(),
            fixture.cid
        ));
    }
    Ok(())
}

fn run_program_proof_check(
    args: &ProofImplementsArgs,
    fixture_path: &std::path::Path,
) -> Result<Json, String> {
    let output = Command::new(&args.program)
        .arg("proof")
        .arg("check")
        .arg(fixture_path)
        .arg("--proof-format-proof")
        .arg(&args.proof_protocol)
        .arg("--policy")
        .arg(&args.policy)
        .arg("--grammar")
        .arg(&args.grammar)
        .arg("--invariants")
        .arg(&args.invariants)
        .arg("--verifier")
        .arg(&args.verifier)
        .arg("--json")
        .output()
        .map_err(|e| format!("spawn program proof check: {e}"))?;
    let parsed: Json = serde_json::from_slice(&output.stdout).map_err(|e| {
        format!(
            "program proof check did not emit JSON: {e}; stdout={}",
            String::from_utf8_lossy(&output.stdout)
        )
    })?;
    let result = parsed
        .get("result")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if !output.status.success() && result {
        return Err(format!(
            "program proof check exited {:?} despite true result: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr),
        ));
    }
    Ok(parsed)
}

fn print_report(report: &ProofFileConformanceReport) {
    println!("{}", "ProvekIt proof inspect".bold());
    println!("  file        : {}", report.proof_path);
    println!("  cid         : {}", report.file_cid.cyan());
    if let Some(filename_cid) = &report.filename_cid {
        println!("  filename CID: {}", filename_cid.cyan());
    }
    println!("  members     : {}", report.member_count);
    println!("  metadata    : {}", report.metadata_count);
    let conformant = if report.ok() {
        "yes".green().to_string()
    } else {
        "no".red().to_string()
    };
    println!("  conformant  : {conformant}");
    if !report.warnings.is_empty() {
        println!();
        println!("  warnings:");
        for warning in &report.warnings {
            println!("    - {warning}");
        }
    }
    if !report.errors.is_empty() {
        println!();
        println!("  errors:");
        for error in &report.errors {
            println!("    - {}: {}", error.rule_id, error.message);
        }
    }
}
