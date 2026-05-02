// SPDX-License-Identifier: Apache-2.0
//
// Lattice fixture generator.
//
// For a fixed seed S and target size N we emit:
//
//   * N contract mementos. Each contract has a forall-quantified
//     pre/post pair over a small synthetic IR (atomic predicates over
//     Int / String / Bool variables), minted via mint_contract with
//     the foundation test key seeded deterministically from
//     (S, "contract", index).
//
//   * ~10*N implication mementos. Each one points (post-of-A,
//     pre-of-B) handshake-style. Antecedent and consequent are drawn
//     from the contract pool by a deterministic LCG so the same seed
//     produces the same lattice byte-for-byte.
//
// Files land at OUTPUT/<aa>/<bb>/<full-cid>.proof, where aa/bb are
// the first two and next two hex chars after the "blake3-512:"
// prefix. This keeps any single directory under ~5000 files even at
// N = 10^6.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use rayon::prelude::*;

use provekit_canonicalizer::Value;
use provekit_claim_envelope::{
    mint_contract, mint_implication, Authoring, MintContractArgs, MintImplicationArgs,
    MintedEnvelope,
};
use provekit_proof_envelope::{
    build_proof_envelope, ed25519_pubkey_string, Ed25519Seed, ProofEnvelopeInput,
};

use crate::GenerateArgs;

const PRODUCED_AT: &str = "2026-04-30T00:00:00.000Z";
const SYNTHETIC_PROVER: &str = "synthetic@1.0";

#[derive(Debug, thiserror::Error)]
pub enum FixtureError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("mint: {0}")]
    Mint(#[from] provekit_claim_envelope::ClaimEnvelopeError),
}

/// Tiny splitmix64 derived from (root_seed, role-tag, index). Used to
/// seed every Ed25519 key and every formula-shape choice
/// deterministically.
fn derive_seed(root: u64, tag: u64, index: u64) -> u64 {
    let mut z = root
        .wrapping_mul(0x9E3779B97F4A7C15)
        .wrapping_add(tag.wrapping_mul(0xBF58476D1CE4E5B9))
        .wrapping_add(index.wrapping_mul(0x94D049BB133111EB));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

fn seed_to_bytes(seed: u64) -> Ed25519Seed {
    let h = blake3::hash(&seed.to_le_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(&h.as_bytes()[..32]);
    out
}

/// One contract memento + the canonical pre/post bytes (for
/// implication wiring). We re-hash pre and post with JCS-canonical
/// bytes to match `preHash`/`postHash` derivation in
/// provekit-claim-envelope.
struct ContractRow {
    cid: String,
    bytes: Vec<u8>,
    pre_hash: String,
    post_hash: String,
}

pub fn generate(args: &GenerateArgs) -> Result<(), FixtureError> {
    if args.size == 0 {
        return Err(FixtureError::Mint(
            provekit_claim_envelope::ClaimEnvelopeError::Other("size must be positive".into()),
        ));
    }

    std::fs::create_dir_all(&args.output)?;

    eprintln!(
        "provekit-showcase: generating lattice  size={} output={} seed={:#x}",
        args.size,
        args.output.display(),
        args.seed
    );
    let t_start = std::time::Instant::now();

    // Phase A: mint N contracts in parallel. Collect to Vec to allow
    // implication wiring in phase B.
    let progress = AtomicUsize::new(0);
    let report_every = (args.size / 20).max(1);
    let contracts: Vec<ContractRow> = (0..args.size)
        .into_par_iter()
        .map(|i| {
            let row = mint_one_contract(args.seed, i as u64);
            let n = progress.fetch_add(1, Ordering::Relaxed) + 1;
            if n % report_every == 0 {
                eprintln!("  contracts: {} / {}", n, args.size);
            }
            row
        })
        .collect();

    // Write all contracts to disk in parallel as .proof envelopes.
    eprintln!("  writing {} contract .proof files...", contracts.len());
    contracts
        .par_iter()
        .try_for_each(|row| write_proof(&args.output, &row.cid, &row.bytes))?;

    // Phase B: mint ~10 * N implications. We pick a (post-of-A,
    // pre-of-B) handshake by indexing into the contract pool with a
    // deterministic LCG. We avoid self-implications.
    let imp_count: usize = args.size.saturating_mul(10);
    eprintln!(
        "  generating {} implications (~10x contracts)...",
        imp_count
    );
    let progress2 = AtomicUsize::new(0);
    let report_every2 = (imp_count / 20).max(1);
    let implications: Vec<MintedEnvelope> = (0..imp_count)
        .into_par_iter()
        .map(|i| {
            let s_a = derive_seed(args.seed, 0xA1, i as u64);
            let s_b = derive_seed(args.seed, 0xA2, i as u64);
            let a_idx = (s_a as usize) % contracts.len();
            let mut b_idx = (s_b as usize) % contracts.len();
            if a_idx == b_idx {
                b_idx = (b_idx + 1) % contracts.len();
            }
            let a = &contracts[a_idx];
            let b = &contracts[b_idx];
            let memento = mint_one_implication(args.seed, i as u64, a, b);
            let n = progress2.fetch_add(1, Ordering::Relaxed) + 1;
            if n % report_every2 == 0 {
                eprintln!("    implications: {} / {}", n, imp_count);
            }
            memento
        })
        .collect();

    eprintln!("  writing {} implication .proof files...", implications.len());
    implications
        .par_iter()
        .try_for_each(|m| write_proof(&args.output, &m.cid, &m.canonical_bytes))?;

    // Compute on-disk size.
    let total: u64 = walkdir::WalkDir::new(&args.output)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
        .sum();

    let dur = t_start.elapsed();
    eprintln!(
        "provekit-showcase: lattice ready  contracts={} implications={} on_disk_bytes={} elapsed={:.2}s",
        contracts.len(),
        implications.len(),
        total,
        dur.as_secs_f64()
    );

    // Drop a tiny manifest with the byte count so benchmark can read it
    // without re-walking.
    let manifest = format!(
        "{{\"contracts\":{},\"implications\":{},\"onDiskBytes\":{},\"seed\":{},\"generatedAt\":\"{}\"}}\n",
        contracts.len(),
        implications.len(),
        total,
        args.seed,
        PRODUCED_AT
    );
    std::fs::write(args.output.join("manifest.json"), manifest)?;
    Ok(())
}

fn mint_one_contract(root_seed: u64, idx: u64) -> ContractRow {
    let key_seed = derive_seed(root_seed, 0xC0, idx);
    let signer_seed = seed_to_bytes(key_seed);
    let signer_id = ed25519_pubkey_string(&signer_seed);

    // Build a forall-shaped pre and post over Int / String / Bool.
    // The integer literal varies per index so the formula JCS bytes
    // (and therefore preHash / postHash / propertyHash) differ.
    let shape = idx % 6;
    let lit_pre = (idx as i64) % 1024;
    let lit_post = (idx as i64) % 4096 + 1;

    let var_n = Value::object([
        ("kind", Value::string("var")),
        ("name", Value::string("n")),
        (
            "sort",
            Value::object([
                ("kind", Value::string("primitive")),
                ("name", Value::string("Int")),
            ]),
        ),
    ]);
    let var_s = Value::object([
        ("kind", Value::string("var")),
        ("name", Value::string("s")),
        (
            "sort",
            Value::object([
                ("kind", Value::string("primitive")),
                ("name", Value::string("String")),
            ]),
        ),
    ]);
    let const_int = |k: i64| {
        Value::object([
            ("kind", Value::string("const")),
            ("value", Value::integer(k)),
            (
                "sort",
                Value::object([
                    ("kind", Value::string("primitive")),
                    ("name", Value::string("Int")),
                ]),
            ),
        ])
    };
    let const_bool = |b: bool| {
        Value::object([
            ("kind", Value::string("const")),
            ("value", Value::boolean(b)),
            (
                "sort",
                Value::object([
                    ("kind", Value::string("primitive")),
                    ("name", Value::string("Bool")),
                ]),
            ),
        ])
    };

    let atomic = |op: &str, args: Vec<Arc<Value>>| {
        Value::object([
            ("kind", Value::string("atomic")),
            ("name", Value::string(op)),
            ("args", Value::array(args)),
        ])
    };
    let forall = |bound: Vec<Arc<Value>>, body: Arc<Value>| {
        Value::object([
            ("kind", Value::string("forall")),
            ("bound", Value::array(bound)),
            ("body", body),
        ])
    };

    let pre = match shape {
        0 => forall(vec![var_n.clone()], atomic(">=", vec![var_n.clone(), const_int(lit_pre)])),
        1 => forall(vec![var_n.clone()], atomic("<=", vec![var_n.clone(), const_int(lit_pre)])),
        2 => forall(vec![var_n.clone()], atomic("=", vec![var_n.clone(), const_int(lit_pre)])),
        3 => forall(vec![var_s.clone()], atomic("string-nonempty?", vec![var_s.clone()])),
        4 => forall(vec![var_n.clone()], atomic(">", vec![var_n.clone(), const_int(lit_pre)])),
        _ => atomic("=", vec![const_bool(true), const_bool(true)]),
    };
    let post = match shape {
        0 => forall(vec![var_n.clone()], atomic(">", vec![var_n.clone(), const_int(lit_post)])),
        1 => forall(vec![var_n.clone()], atomic("<", vec![var_n.clone(), const_int(lit_post)])),
        2 => forall(vec![var_n.clone()], atomic("=", vec![var_n.clone(), const_int(lit_post + 1)])),
        3 => forall(vec![var_s.clone()], atomic("string-prefix?", vec![var_s.clone(), Value::string("foo")])),
        4 => forall(vec![var_n.clone()], atomic(">=", vec![var_n.clone(), const_int(lit_post)])),
        _ => atomic("=", vec![const_bool(true), const_bool(true)]),
    };

    let mint = mint_contract(&MintContractArgs {
        contract_name: format!("synthContract_{idx}"),
        pre: Some(pre.clone()),
        post: Some(post.clone()),
        inv: None,
        out_binding: format!("out_{idx}"),
        produced_by: signer_id.clone(),
        produced_at: PRODUCED_AT.to_string(),
        input_cids: vec![],
        authoring: Authoring::KitAuthor {
            author: SYNTHETIC_PROVER.to_string(),
            note: Some("provekit-showcase synthetic fixture".into()),
        },
        signer_seed,
    })
    .expect("synthetic contract is well-formed");

    let pre_hash = hash_value(&pre);
    let post_hash = hash_value(&post);

    // Wrap the minted memento as a one-member catalog so it is itself
    // a self-identifying .proof bundle.
    let proof = wrap_as_proof(
        &format!("synthetic.contract.{idx}"),
        &signer_id,
        &signer_seed,
        &mint,
    );

    ContractRow {
        cid: proof.cid,
        bytes: proof.bytes,
        pre_hash,
        post_hash,
    }
}

fn mint_one_implication(
    root_seed: u64,
    idx: u64,
    a: &ContractRow,
    b: &ContractRow,
) -> MintedEnvelope {
    let key_seed = derive_seed(root_seed, 0xD0, idx);
    let signer_seed = seed_to_bytes(key_seed);
    let signer_id = ed25519_pubkey_string(&signer_seed);

    let mint = mint_implication(&MintImplicationArgs {
        produced_by: signer_id.clone(),
        produced_at: PRODUCED_AT.to_string(),
        antecedent_hash: a.post_hash.clone(),
        consequent_hash: b.pre_hash.clone(),
        antecedent_cid: a.cid.clone(),
        consequent_cid: b.cid.clone(),
        antecedent_slot: "post".to_string(),
        consequent_slot: "pre".to_string(),
        prover: SYNTHETIC_PROVER.to_string(),
        prover_run_ms: 0,
        smt_lib_input: String::new(),
        proof_witness: String::new(),
        signer_seed,
    });

    // Wrap the implication as its own .proof bundle.
    let proof = wrap_as_proof(
        &format!("synthetic.implication.{idx}"),
        &signer_id,
        &signer_seed,
        &mint,
    );
    MintedEnvelope {
        canonical_bytes: proof.bytes,
        cid: proof.cid,
    }
}

fn wrap_as_proof(
    name: &str,
    signer_cid: &str,
    signer_seed: &Ed25519Seed,
    member: &MintedEnvelope,
) -> provekit_proof_envelope::ProofEnvelopeOutput {
    let mut members = BTreeMap::new();
    members.insert(member.cid.clone(), member.canonical_bytes.clone());
    let input = ProofEnvelopeInput {
        name: name.to_string(),
        version: "1.0.0".to_string(),
        binary_cid: None,
        metadata: None,
        members,
        signer_cid: signer_cid.to_string(),
        signer_seed: *signer_seed,
        declared_at: PRODUCED_AT.to_string(),
    };
    build_proof_envelope(&input)
}

fn hash_value(v: &Arc<Value>) -> String {
    let bytes = provekit_canonicalizer::encode_jcs(v);
    provekit_canonicalizer::blake3_512_of(bytes.as_bytes())
}

fn write_proof(root: &Path, cid: &str, bytes: &[u8]) -> std::io::Result<()> {
    let (a, b) = shard_for(cid);
    let mut path: PathBuf = root.into();
    path.push(a);
    path.push(b);
    std::fs::create_dir_all(&path)?;
    path.push(format!("{cid}.proof"));
    std::fs::write(path, bytes)
}

fn shard_for(cid: &str) -> (&str, &str) {
    // CID is "blake3-512:<128 hex>". Use first 2 and next 2 hex chars
    // as shard keys.
    let stripped = cid.strip_prefix("blake3-512:").unwrap_or(cid);
    let bytes = stripped.as_bytes();
    let a = if bytes.len() >= 2 {
        std::str::from_utf8(&bytes[0..2]).unwrap_or("00")
    } else {
        "00"
    };
    let b = if bytes.len() >= 4 {
        std::str::from_utf8(&bytes[2..4]).unwrap_or("00")
    } else {
        "00"
    };
    (a, b)
}
