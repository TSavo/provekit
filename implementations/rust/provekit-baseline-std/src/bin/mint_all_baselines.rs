//! mint-all-baselines: config-driven baseline catalog generator.
//! Reads per-language TOML configs, generates ContractDecls using
//! provekit-ir-symbolic primitives, mints each into a signed memento,
//! bundles into per-kit content-addressed .proof files under .provekit/baselines/.

use std::fs;
use std::path::Path;
use std::rc::Rc;

use provekit_claim_envelope::{mint_contract, Authoring, MintContractArgs};
use provekit_ir_symbolic::serialize::formula_to_value;
use provekit_ir_symbolic::{eq, forall, gte, num, reset_collector, str_const, String_, Term};
use provekit_proof_envelope::{
    build_proof_envelope, ed25519_pubkey_string, Ed25519Seed, ProofEnvelopeInput,
};

use serde::Deserialize;

const FOUNDATION_V0_SEED: Ed25519Seed = [0x42u8; 32];

#[allow(dead_code)] // nullary ctor; not used by current predicate set but kept for future extensibility
fn ctor0(name: &str) -> Rc<Term> {
    Rc::new(Term::Ctor {
        name: name.into(),
        args: vec![],
    })
}
fn ctor1(name: &str, arg: Rc<Term>) -> Rc<Term> {
    Rc::new(Term::Ctor {
        name: name.into(),
        args: vec![arg],
    })
}
#[allow(dead_code)] // binary ctor; not used by current predicate set but kept for future extensibility
fn ctor2(name: &str, a: Rc<Term>, b: Rc<Term>) -> Rc<Term> {
    Rc::new(Term::Ctor {
        name: name.into(),
        args: vec![a, b],
    })
}
fn ctor_n(name: &str, args: Vec<Rc<Term>>) -> Rc<Term> {
    Rc::new(Term::Ctor {
        name: name.into(),
        args,
    })
}

// ── Config types ────────────────────────────────────

#[derive(Deserialize)]
struct BaselineConfig {
    lang: String,
    kit: String,
    #[allow(dead_code)]
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

// ── IR helper: apply builtin sig to quantified vars ──

fn apply_builtin(sig: &str, arity: usize, vars: &[Rc<Term>]) -> Rc<Term> {
    // var0..varN correspond to quantified variables in order.
    // For arity=0: nullary ctor: no arguments (e.g. a constant).
    // For arity=1: ctor1(sig, var0).
    // For arity>=2: ctorN(sig, var0..varN).
    // Default (unconfigured arity=0): treat as arity=1 (unary function).
    if arity == 0 {
        // arity field absent from config (serde default 0) means unary, not truly nullary.
        // Callers that want a nullary ctor must pass explicit arity=0 AND a known-nullary sig.
        // Since all builtins in practice take at least one argument, treat absent=unary.
        ctor1(
            sig,
            vars.first()
                .cloned()
                .unwrap_or_else(|| Rc::new(Term::Var { name: "_".into() })),
        )
    } else if arity == 1 || vars.len() < 2 {
        ctor1(
            sig,
            vars.first()
                .cloned()
                .unwrap_or_else(|| Rc::new(Term::Var { name: "_".into() })),
        )
    } else {
        ctor_n(sig, vars[..arity.min(vars.len())].to_vec())
    }
}

// ── Mint infrastructure ─────────────────────────────

fn mint_baseline(config: &BaselineConfig, out_dir: &Path) -> String {
    // Reset per-language quantifier counter so CIDs don't cross-contaminate.
    reset_collector();

    let produced_by = "provekit-baseline-std@0.1.0".to_string();
    let produced_at = "2026-05-04T00:00:00Z".to_string();

    let mut members = std::collections::BTreeMap::new();
    let mut content_cids: Vec<String> = Vec::new();

    for builtin in &config.builtins {
        let builtin_name = &builtin.name;
        let sig = builtin.signature.clone();
        let ret_type = builtin.return_type.clone();
        let arity = builtin.arity;

        for pred in &builtin.predicates {
            let (cname, formula) = match pred.kind.as_str() {
                "type_signature" => {
                    let sig_f = sig.clone();
                    let rt = ret_type.clone();
                    let f = if arity <= 1 {
                        forall(String_(), move |s| {
                            eq(
                                ctor1("type_of", apply_builtin(&sig_f, arity, &[s])),
                                str_const(&rt),
                            )
                        })
                    } else {
                        // Multi-arity: forall multiple vars
                        forall(String_(), move |s0| {
                            forall(String_(), move |s1| {
                                eq(
                                    ctor1("type_of", apply_builtin(&sig_f, arity, &[s0, s1])),
                                    str_const(&rt),
                                )
                            })
                        })
                    };
                    (format!("{builtin_name}__type_signature"), f)
                }
                "determinism" => {
                    let sig_f = sig.clone();
                    let f = if arity <= 1 {
                        forall(String_(), move |s| {
                            eq(
                                apply_builtin(&sig_f, arity, &[s.clone()]),
                                apply_builtin(&sig_f, arity, &[s]),
                            )
                        })
                    } else {
                        forall(String_(), move |s0| {
                            forall(String_(), move |s1| {
                                eq(
                                    apply_builtin(&sig_f, arity, &[s0.clone(), s1.clone()]),
                                    apply_builtin(&sig_f, arity, &[s0, s1]),
                                )
                            })
                        })
                    };
                    (format!("{builtin_name}__determinism"), f)
                }
                "gte" => {
                    // `left` is the property function applied directly to the input variable s.
                    // e.g. left="str_len" right=0 means: forall s, str_len(s) >= 0.
                    // The `left` name IS the function; it takes s as its argument.
                    // NEVER double-wrap: ctor1(left, ctor1(sig, s)) is always wrong for gte.
                    let left_name = pred
                        .args
                        .as_ref()
                        .and_then(|a| {
                            if a.left.is_empty() {
                                None
                            } else {
                                Some(a.left.clone())
                            }
                        })
                        .unwrap_or_else(|| {
                            panic!(
                                "gte predicate for {}/{} requires non-empty `left` field in config",
                                config.lang, builtin_name
                            )
                        });
                    let right_val = pred.args.as_ref().map(|a| a.right).unwrap_or_else(|| {
                        panic!(
                            "gte predicate for {}/{} requires `right` field in config",
                            config.lang, builtin_name
                        )
                    });
                    let f = forall(String_(), move |s| {
                        // left_name(s) >= right_val: the property bounded below.
                        let inner = ctor1(&left_name, s);
                        gte(inner, num(right_val))
                    });
                    (format!("{builtin_name}__structural"), f)
                }
                other => {
                    panic!(
                        "unknown predicate kind '{}' for {}/{}: fix config or add handler",
                        other, config.lang, builtin_name
                    );
                }
            };

            let args = MintContractArgs {
                formals: Vec::new(),
                formal_sorts: Vec::new(),
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
            members
                .entry(minted.cid.clone())
                .or_insert(minted.canonical_bytes);
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

    fs::create_dir_all(out_dir).expect("create baseline output dir");
    let out_path = out_dir.join(format!("{}.proof", built.cid));
    fs::write(&out_path, &built.bytes).expect("write .proof");
    println!("  {}/{}  {}", config.lang, built.cid, out_path.display());

    built.cid
}

fn main() {
    let config_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("configs");
    let out_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../.provekit/baselines");

    // 11 language kits. Rust is handled by provekit-baseline-rust-std (#292).
    // Including rust here would produce a second rust baseline artifact.
    let langs = [
        "c",
        "cpp",
        "csharp",
        "go",
        "java",
        "php",
        "python",
        "ruby",
        "swift",
        "typescript",
        "zig",
    ];

    let mut count = 0;
    for lang in langs {
        let config_path = config_dir.join(format!("{lang}.toml"));
        if !config_path.exists() {
            panic!(
                "required config missing: {}: every language in the langs list must have a config file",
                config_path.display()
            );
        }
        let toml_text =
            fs::read_to_string(&config_path).unwrap_or_else(|e| panic!("read {lang} config: {e}"));
        let config: BaselineConfig =
            toml::from_str(&toml_text).unwrap_or_else(|e| panic!("parse {lang} config: {e}"));
        mint_baseline(&config, &out_dir);
        count += 1;
    }

    if count == 0 {
        panic!(
            "no configs found in {}: nothing to mint",
            config_dir.display()
        );
    }

    println!("\nminted {count} baselines to {}", out_dir.display());
}
