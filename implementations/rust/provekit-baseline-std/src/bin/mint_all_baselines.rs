//! mint-all-baselines — config-driven baseline catalog generator.
//! Reads per-language TOML configs, generates ContractDecls using
//! provekit-ir-symbolic primitives, mints each into a signed memento,
//! bundles into per-kit .proof files under .provekit/baselines/.

use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use provekit_ir_symbolic as ir;
use provekit_ir_symbolic::{
    contract, eq, forall, gte, must, num, str_const, ContractArgs, String_, Term,
};

fn ctor1(name: &str, arg: Rc<Term>) -> Rc<Term> {
    Rc::new(Term::Ctor { name: name.into(), args: vec![arg] })
}
fn ctor2(name: &str, a: Rc<Term>, b: Rc<Term>) -> Rc<Term> {
    Rc::new(Term::Ctor { name: name.into(), args: vec![a, b] })
}

// ── Config types ────────────────────────────────────

use serde::Deserialize;

#[derive(Deserialize)]
struct BaselineConfig {
    lang: String,
    kit: String,
    language_version: String,
    builtins: Vec<BuiltinConfig>,
}

#[derive(Deserialize)]
struct BuiltinConfig {
    name: String,
    signature: String,
    #[serde(default)]
    arity: usize,
    return_type: String,
    #[serde(default)]
    predicates: Vec<PredicateConfig>,
}

#[derive(Deserialize)]
struct PredicateConfig {
    kind: String,
    #[serde(default)]
    args: Option<PredicateArgs>,
}

#[derive(Deserialize, Default)]
struct PredicateArgs {
    #[serde(default)]
    left: String,
    #[serde(default)]
    right: i64,
    #[serde(default)]
    comment: String,
}

// ── Mint infrastructure ─────────────────────────────

use provekit_canonicalizer::{Ed25519Seed, FOUNDATION_V0_SEED};
use provekit_claim_envelope::{mint_contract, Authoring, MintContractArgs};
use provekit_proof_envelope::{build_proof_envelope, ProofEnvelopeInput};

fn mint_baseline(config: &BaselineConfig, out_dir: &Path) -> String {
    let signer_seed = FOUNDATION_V0_SEED;
    let produced_by = format!("provekit-baseline-std@0.1.0");
    let produced_at = "2026-05-04T00:00:00Z";

    let mut members = std::collections::BTreeMap::new();
    let mut content_cids: Vec<String> = Vec::new();

    for builtin in &config.builtins {
        let builtin_name = &builtin.name;
        let sig = &builtin.signature;
        let ret_type = &builtin.return_type;

        // Build formulas from predicate config
        for pred in &builtin.predicates {
            let (cname, formula) = match pred.kind.as_str() {
                "type_signature" => {
                    let f = forall(String_(), |s| {
                        eq(ctor1("type_of", ctor1(sig, s)), str_const(ret_type))
                    });
                    (format!("{builtin_name}__type_signature"), f)
                }
                "determinism" => {
                    let f = forall(String_(), |s| {
                        eq(ctor1(sig, s.clone()), ctor1(sig, s))
                    });
                    (format!("{builtin_name}__determinism"), f)
                }
                "gte" => {
                    let left_name = pred.args.as_ref().map(|a| a.left.clone()).unwrap_or_default();
                    let right_val = pred.args.as_ref().map(|a| a.right).unwrap_or(0);
                    let f = forall(String_(), |s| {
                        gte(ctor1(&left_name, ctor1(sig, s)), num(right_val))
                    });
                    (format!("{builtin_name}__structural"), f)
                }
                _ => continue,
            };

            // Determine which slot to use (pre/post/inv)
            let args = MintContractArgs {
                contract_name: cname,
                pre: None,
                post: Some(formula),
                inv: None,
                out_binding: "out".into(),
                produced_by: produced_by.clone(),
                produced_at: produced_at.clone(),
                input_cids: vec![],
                authoring: Authoring::KitAuthor {
                    author: format!("{}-kit-baseline", config.kit),
                    note: format!("std {} baseline v1", config.lang),
                },
                signer_seed,
            };

            let minted = mint_contract(&args).expect("mint contract");
            content_cids.push(minted.cid.clone());
            members.entry(minted.cid).or_insert(minted.canonical_bytes);
        }
    }

    if members.is_empty() {
        eprintln!("warning: no contracts minted for {}", config.lang);
        return String::new();
    }

    let signer_pubkey = provekit_proof_envelope::ed25519_pubkey_string(&signer_seed);
    let signer_cid = provekit_canonicalizer::blake3_512_of(signer_pubkey.as_bytes());

    let proof_input = ProofEnvelopeInput {
        name: format!("{}-std-baseline-v1", config.lang),
        version: "1.0.0".into(),
        binary_cid: None,
        metadata: None,
        members,
        signer_cid,
        signer_seed,
        declared_at: produced_at.to_string(),
    };

    let built = build_proof_envelope(&proof_input);

    fs::create_dir_all(out_dir).ok();
    let out_path = out_dir.join(format!("{}.proof", built.cid));
    fs::write(&out_path, &built.bytes).expect("write .proof");

    built.cid
}

fn main() {
    let config_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("configs");
    let out_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../.provekit/baselines");

    let langs = [
        "c", "cpp", "csharp", "go", "java", "php", "python",
        "ruby", "swift", "typescript", "zig",
    ];

    for lang in langs {
        let config_path = config_dir.join(format!("{lang}.toml"));
        if !config_path.exists() {
            eprintln!("skip: no config for {lang}");
            continue;
        }
        let toml_text = fs::read_to_string(&config_path).expect("read config");
        let config: BaselineConfig = toml::from_str(&toml_text).expect("parse config");
        let cid = mint_baseline(&config, &out_dir);
        println!("{lang}: {cid}");
    }
}
