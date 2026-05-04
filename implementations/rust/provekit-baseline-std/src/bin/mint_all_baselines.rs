//! mint-all-baselines — config-driven baseline catalog generator.
//! Reads per-language TOML configs, generates ContractDecls using
//! provekit-ir-symbolic primitives, mints each into a signed memento,
//! bundles into per-kit .proof files under .provekit/baselines/.

use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use provekit_ir_symbolic::{contract, eq, forall, gte, num, str_const, ContractArgs, String_, Term};
use provekit_ir_symbolic::serialize::formula_to_value;
use provekit_proof_envelope::{build_proof_envelope, ed25519_pubkey_string, Ed25519Seed, ProofEnvelopeInput};
use provekit_claim_envelope::{mint_contract, Authoring, MintContractArgs};

use serde::Deserialize;

const FOUNDATION_V0_SEED: Ed25519Seed = [0x42u8; 32];

fn ctor1(name: &str, arg: Rc<Term>) -> Rc<Term> {
    Rc::new(Term::Ctor { name: name.into(), args: vec![arg] })
}

// ── Config types ────────────────────────────────────

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
}

// ── Mint infrastructure ─────────────────────────────

fn mint_baseline(config: &BaselineConfig, out_dir: &Path) -> String {
    let produced_by = "provekit-baseline-std@0.1.0".to_string();
    let produced_at = "2026-05-04T00:00:00Z".to_string();

    let mut members = std::collections::BTreeMap::new();
    let mut content_cids: Vec<String> = Vec::new();

    for builtin in &config.builtins {
        let builtin_name = &builtin.name;
        let sig = builtin.signature.clone();
        let ret_type = builtin.return_type.clone();

        for pred in &builtin.predicates {
            let (cname, formula) = match pred.kind.as_str() {
                "type_signature" => {
                    let sig_f = sig.clone();
                    let rt = ret_type.clone();
                    let f = forall(String_(), move |s| {
                        eq(ctor1("type_of", ctor1(&sig_f, s)), str_const(&rt))
                    });
                    (format!("{builtin_name}__type_signature"), f)
                }
                "determinism" => {
                    let sig_f = sig.clone();
                    let f = forall(String_(), move |s| {
                        eq(ctor1(&sig_f, s.clone()), ctor1(&sig_f, s))
                    });
                    (format!("{builtin_name}__determinism"), f)
                }
                "gte" => {
                    let left_name = pred.args.as_ref().map(|a| a.left.clone()).unwrap_or_default();
                    let right_val = pred.args.as_ref().map(|a| a.right).unwrap_or(0);
                    let sig_f = sig.clone();
                    let f = forall(String_(), move |s| {
                        gte(ctor1(&left_name, ctor1(&sig_f, s)), num(right_val))
                    });
                    (format!("{builtin_name}__structural"), f)
                }
                _ => continue,
            };

            let args = MintContractArgs {
                contract_name: cname,
                pre: None,
                post: Some(formula_to_value(&formula)),
                inv: None,
                out_binding: "out".into(),
                produced_by: produced_by.clone(),
                produced_at: produced_at.clone(),
                input_cids: vec![],
                authoring: Authoring::KitAuthor {
                    author: format!("{}-kit-baseline", config.kit),
                    note: Some(format!("std {} baseline v1", config.lang)),
                },
                signer_seed: FOUNDATION_V0_SEED,
            };

            let minted = mint_contract(&args).expect("mint contract");
            content_cids.push(minted.cid.clone());
            members.entry(minted.cid.clone()).or_insert(minted.canonical_bytes);
        }
    }

    if members.is_empty() {
        eprintln!("warning: no contracts minted for {}", config.lang);
        return String::new();
    }

    let signer_pubkey = ed25519_pubkey_string(&FOUNDATION_V0_SEED);
    let signer_cid = provekit_canonicalizer::blake3_512_of(signer_pubkey.as_bytes());

    let proof_input = ProofEnvelopeInput {
        name: format!("{}-std-baseline-v1", config.lang),
        version: "1.0.0".into(),
        binary_cid: None,
        metadata: None,
        members,
        signer_cid,
        signer_seed: FOUNDATION_V0_SEED,
        declared_at: produced_at,
    };

    let built = build_proof_envelope(&proof_input);

    fs::create_dir_all(out_dir).ok();
    let out_path = out_dir.join(format!("{}.proof", built.cid));
    fs::write(&out_path, &built.bytes).expect("write .proof");
    println!("  {}/{} -> {}", config.lang, built.cid, out_path.display());

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

    let mut count = 0;
    for lang in langs {
        let config_path = config_dir.join(format!("{lang}.toml"));
        if !config_path.exists() {
            eprintln!("skip: no config for {lang}");
            continue;
        }
        let toml_text = fs::read_to_string(&config_path).unwrap_or_else(|e| {
            panic!("read {lang} config: {e}");
        });
        let config: BaselineConfig = toml::from_str(&toml_text).unwrap_or_else(|e| {
            panic!("parse {lang} config: {e}");
        });
        mint_baseline(&config, &out_dir);
        count += 1;
    }

    println!("\nminted {count} baselines to {}", out_dir.display());
}
