// SPDX-License-Identifier: Apache-2.0
//
// `provekit protocol ...`: protocol-catalog evolution workflow.
//
// The core verifier does not execute PEP. This command is extension-aware
// tooling: it builds or checks a signed-byte-graph-shaped claim that a
// catalog transition is admissible under a named policy.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::{Parser, Subcommand};
use owo_colors::OwoColorize;
use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CValue};
use serde_json::{json, Value as Json};

use crate::protocol::verify_signed_attestation;
use crate::OutputFlags;

#[derive(Parser, Debug, Clone)]
pub struct ProtocolArgs {
    #[command(subcommand)]
    pub cmd: ProtocolCmd,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ProtocolCmd {
    /// Emit a PEP body and TDP-shaped witness for a catalog transition.
    Evolve(ProtocolEvolveArgs),
    /// Check an already-built PEP body against its evidence bytes.
    CheckEvolution(ProtocolCheckEvolutionArgs),
}

#[derive(Parser, Debug, Clone)]
pub struct ProtocolEvolveArgs {
    /// Predecessor protocol catalog JSON.
    #[arg(long = "from")]
    pub from_catalog: PathBuf,
    /// Successor protocol catalog JSON.
    #[arg(long = "to")]
    pub to_catalog: PathBuf,
    /// Changed spec binding, formatted as `property-key=path/to/spec`.
    #[arg(long = "changed-spec")]
    pub changed_specs: Vec<String>,
    /// Protocol evolution policy JSON artifact.
    #[arg(long)]
    pub policy: PathBuf,
    /// Protocol evolution verifier JSON artifact.
    #[arg(long)]
    pub verifier: PathBuf,
    /// Optional signed catalog attestation JSON for the successor catalog.
    #[arg(long)]
    pub attestation: Option<PathBuf>,
    /// Directory where PEP artifacts are written.
    #[arg(long)]
    pub out_dir: PathBuf,
    /// Declared PEP change class.
    #[arg(long, default_value = "extension-only")]
    pub change_class: String,
    /// Producer name recorded in the body and witness metadata.
    #[arg(long, default_value = "provekit protocol evolve")]
    pub producer: String,
    #[command(flatten)]
    pub out: OutputFlags,
}

#[derive(Parser, Debug, Clone)]
pub struct ProtocolCheckEvolutionArgs {
    /// Existing ProtocolEvolutionBodyClaim JSON.
    #[arg(long)]
    pub body: PathBuf,
    /// Predecessor protocol catalog JSON.
    #[arg(long = "from")]
    pub from_catalog: PathBuf,
    /// Successor protocol catalog JSON.
    #[arg(long = "to")]
    pub to_catalog: PathBuf,
    /// Protocol evolution policy JSON artifact.
    #[arg(long)]
    pub policy: PathBuf,
    /// Protocol evolution verifier JSON artifact.
    #[arg(long)]
    pub verifier: PathBuf,
    /// Catalog diff JSON artifact named by the PEP body evidence.
    #[arg(long = "catalog-diff")]
    pub catalog_diff: PathBuf,
    /// Optional signed catalog attestation JSON for the successor catalog.
    #[arg(long)]
    pub attestation: Option<PathBuf>,
    #[command(flatten)]
    pub out: OutputFlags,
}

pub fn run(args: ProtocolArgs) -> u8 {
    match args.cmd {
        ProtocolCmd::Evolve(args) => match run_evolve(args) {
            Ok(summary) => {
                if summary.out.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&summary.payload)
                            .unwrap_or_else(|_| summary.payload.to_string())
                    );
                } else if !summary.out.quiet {
                    println!("{}", "ProvekIt protocol evolution".bold());
                    println!("  from catalog : {}", summary.from_catalog_cid);
                    println!("  to catalog   : {}", summary.to_catalog_cid);
                    println!("  body CID     : {}", summary.body_cid);
                    println!("  witness CID  : {}", summary.witness_cid);
                    println!("  out dir      : {}", summary.out_dir.display());
                }
                crate::EXIT_OK
            }
            Err(e) => {
                eprintln!("{}: {e}", "error".red().bold());
                crate::EXIT_USER_ERROR
            }
        },
        ProtocolCmd::CheckEvolution(args) => match run_check_evolution(args) {
            Ok(summary) => {
                if summary.out.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&summary.payload)
                            .unwrap_or_else(|_| summary.payload.to_string())
                    );
                } else if !summary.out.quiet {
                    println!("{}", "ProvekIt protocol evolution check".bold());
                    println!("  body CID     : {}", summary.body_cid);
                    println!("  from catalog : {}", summary.from_catalog_cid);
                    println!("  to catalog   : {}", summary.to_catalog_cid);
                    println!("  status       : {}", "admitted".green().bold());
                }
                crate::EXIT_OK
            }
            Err(e) => {
                eprintln!("{}: {e}", "error".red().bold());
                crate::EXIT_USER_ERROR
            }
        },
    }
}

struct EvolveSummary {
    payload: Json,
    from_catalog_cid: String,
    to_catalog_cid: String,
    body_cid: String,
    witness_cid: String,
    out_dir: PathBuf,
    out: OutputFlags,
}

struct CheckEvolutionSummary {
    payload: Json,
    body_cid: String,
    from_catalog_cid: String,
    to_catalog_cid: String,
    out: OutputFlags,
}

fn run_evolve(args: ProtocolEvolveArgs) -> Result<EvolveSummary, String> {
    fs::create_dir_all(&args.out_dir)
        .map_err(|e| format!("mkdir {}: {e}", args.out_dir.display()))?;

    let from = read_json(&args.from_catalog)?;
    let to = read_json(&args.to_catalog)?;
    let policy = read_json(&args.policy)?;
    let verifier = read_json(&args.verifier)?;

    let from_catalog_cid = jcs_cid(&from)?;
    let to_catalog_cid = jcs_cid(&to)?;
    if from_catalog_cid == to_catalog_cid {
        return Err("from and to catalog CIDs are identical; no evolution edge exists".into());
    }

    let from_version = catalog_string(&from, "version")?;
    let to_version = catalog_string(&to, "version")?;
    let protocol_name = catalog_string(&to, "name")?;
    if catalog_string(&from, "name")? != protocol_name {
        return Err("from and to catalogs have different protocol names".into());
    }

    let changed_specs = parse_changed_specs(&args.changed_specs)?;
    let from_props = catalog_properties(&from)?;
    let to_props = catalog_properties(&to)?;
    let diff = compute_diff(&from_props, &to_props, &changed_specs, &args.change_class)?;
    if diff.added.is_empty() && diff.modified.is_empty() && diff.removed.is_empty() {
        return Err("catalog properties are unchanged; no evolution edge exists".into());
    }

    let catalog_diff = json!({
        "kind": "ProtocolCatalogDiff",
        "schemaVersion": "1",
        "protocolName": protocol_name,
        "fromCatalogCid": from_catalog_cid,
        "toCatalogCid": to_catalog_cid,
        "fromVersionLabel": from_version,
        "toVersionLabel": to_version,
        "changeClass": args.change_class,
        "added": diff.added,
        "modified": diff.modified,
        "removed": diff.removed,
        "deprecated": [],
        "unchangedSemantics": diff.unchanged_semantics,
    });
    let catalog_diff_cid = jcs_cid(&catalog_diff)?;
    write_json(&args.out_dir.join("catalog-diff.json"), &catalog_diff)?;

    let policy_cid = jcs_cid(&policy)?;
    let verifier_cid = jcs_cid(&verifier)?;
    let catalog_attestation = match args.attestation.as_ref() {
        Some(path) => Some(read_json_bytes(path)?),
        None => None,
    };
    let catalog_attestation_cid = match catalog_attestation.as_ref() {
        Some((json, _bytes)) => Some(jcs_cid(json)?),
        None => None,
    };

    let mut input_cids = vec![
        from_catalog_cid.clone(),
        to_catalog_cid.clone(),
        catalog_diff_cid.clone(),
        verifier_cid.clone(),
        policy_cid.clone(),
    ];
    input_cids.extend(diff.changed_spec_cids.iter().cloned());
    let mut diff_cids = BTreeSet::new();
    collect_owned_cids(&catalog_diff, &mut diff_cids);
    input_cids.extend(diff_cids);
    if let Some(cid) = &catalog_attestation_cid {
        input_cids.push(cid.clone());
    }
    input_cids.sort();
    input_cids.dedup();

    let body = json!({
        "kind": "ProtocolEvolutionBodyClaim",
        "schemaVersion": "1",
        "protocolName": catalog_string(&to, "name")?,
        "fromCatalogCid": from_catalog_cid,
        "toCatalogCid": to_catalog_cid,
        "fromVersionLabel": catalog_string(&from, "version")?,
        "toVersionLabel": catalog_string(&to, "version")?,
        "changeClass": args.change_class,
        "changeSet": {
            "added": diff.added,
            "modified": diff.modified,
            "removed": diff.removed,
            "deprecated": []
        },
        "compatibility": {
            "claim": "backward-compatible",
            "migrationRequired": false,
            "bridgeWitnessCids": [],
            "migrationWitnessCids": []
        },
        "evidence": {
            "catalogDiffCid": catalog_diff_cid,
            "catalogAttestationCid": catalog_attestation_cid,
            "grammarConformanceWitnessCid": null,
            "invariantConformanceWitnessCid": null,
            "conformanceCorpusCid": null,
            "conformanceWitnessCids": [],
            "implementationAdoptionWitnessCids": [],
            "fixReceiptCids": []
        },
        "verifierCid": verifier_cid,
        "policyCid": policy_cid,
        "inputCids": input_cids,
        "producer": {
            "kind": "automation",
            "name": args.producer,
            "version": env!("CARGO_PKG_VERSION")
        }
    });
    admit_protocol_evolution(
        &from,
        &to,
        &policy,
        &verifier,
        catalog_attestation
            .as_ref()
            .map(|(_json, bytes)| bytes.as_slice()),
        &catalog_diff,
        &body,
    )?;

    let body_cid = jcs_cid(&body)?;
    write_json(&args.out_dir.join("protocol-evolution.body.json"), &body)?;
    write_text(
        &args.out_dir.join("protocol-evolution.body.cid.txt"),
        &format!("{body_cid}\n"),
    )?;

    let witness_inputs = collect_witness_inputs(&body, &body_cid);
    let witness = json!({
        "kind": "TruthDischargeWitness",
        "schemaVersion": "1",
        "claimBodyCid": body_cid,
        "claimKind": "protocol-evolution",
        "result": true,
        "verifierCid": body["verifierCid"].clone(),
        "policyCid": body["policyCid"].clone(),
        "evidenceRootCid": body["evidence"]["catalogDiffCid"].clone(),
        "inputCids": witness_inputs,
        "execution": {
            "startedAt": "1970-01-01T00:00:00Z",
            "finishedAt": "1970-01-01T00:00:00Z",
            "fuelUsed": 0
        },
        "metadata": {
            "producer": args.producer,
            "producerVersion": env!("CARGO_PKG_VERSION"),
            "bootstrapNote": "TDP-shaped PEP witness emitted by extension-aware tooling; core verification does not execute PEP."
        }
    });
    let witness_cid = jcs_cid(&witness)?;
    write_json(
        &args.out_dir.join("protocol-evolution.witness.json"),
        &witness,
    )?;
    write_text(
        &args.out_dir.join("protocol-evolution.witness.cid.txt"),
        &format!("{witness_cid}\n"),
    )?;

    let payload = json!({
        "kind": "ProtocolEvolutionSummary",
        "fromCatalogCid": body["fromCatalogCid"].clone(),
        "toCatalogCid": body["toCatalogCid"].clone(),
        "fromVersionLabel": body["fromVersionLabel"].clone(),
        "toVersionLabel": body["toVersionLabel"].clone(),
        "changeClass": body["changeClass"].clone(),
        "catalogDiffCid": body["evidence"]["catalogDiffCid"].clone(),
        "bodyCid": body_cid,
        "witnessCid": witness_cid,
        "outDir": args.out_dir.display().to_string(),
        "bodyPath": args.out_dir.join("protocol-evolution.body.json").display().to_string(),
        "witnessPath": args.out_dir.join("protocol-evolution.witness.json").display().to_string(),
    });

    Ok(EvolveSummary {
        from_catalog_cid: payload["fromCatalogCid"].as_str().unwrap().to_string(),
        to_catalog_cid: payload["toCatalogCid"].as_str().unwrap().to_string(),
        body_cid: payload["bodyCid"].as_str().unwrap().to_string(),
        witness_cid: payload["witnessCid"].as_str().unwrap().to_string(),
        out_dir: args.out_dir,
        out: args.out,
        payload,
    })
}

fn run_check_evolution(args: ProtocolCheckEvolutionArgs) -> Result<CheckEvolutionSummary, String> {
    let body = read_json(&args.body)?;
    let from = read_json(&args.from_catalog)?;
    let to = read_json(&args.to_catalog)?;
    let policy = read_json(&args.policy)?;
    let verifier = read_json(&args.verifier)?;
    let catalog_diff = read_json(&args.catalog_diff)?;
    let catalog_attestation = match args.attestation.as_ref() {
        Some(path) => Some(read_json_bytes(path)?),
        None => None,
    };

    admit_protocol_evolution(
        &from,
        &to,
        &policy,
        &verifier,
        catalog_attestation
            .as_ref()
            .map(|(_json, bytes)| bytes.as_slice()),
        &catalog_diff,
        &body,
    )?;

    let body_cid = jcs_cid(&body)?;
    let from_catalog_cid = jcs_cid(&from)?;
    let to_catalog_cid = jcs_cid(&to)?;
    let payload = json!({
        "kind": "ProtocolEvolutionCheck",
        "ok": true,
        "bodyCid": body_cid,
        "fromCatalogCid": from_catalog_cid,
        "toCatalogCid": to_catalog_cid,
        "changeClass": body["changeClass"].clone(),
        "catalogDiffCid": body["evidence"]["catalogDiffCid"].clone(),
        "policyCid": body["policyCid"].clone(),
        "verifierCid": body["verifierCid"].clone(),
    });

    Ok(CheckEvolutionSummary {
        body_cid: payload["bodyCid"].as_str().unwrap().to_string(),
        from_catalog_cid: payload["fromCatalogCid"].as_str().unwrap().to_string(),
        to_catalog_cid: payload["toCatalogCid"].as_str().unwrap().to_string(),
        out: args.out,
        payload,
    })
}

#[derive(Debug)]
struct CatalogDiff {
    added: Vec<Json>,
    modified: Vec<Json>,
    removed: Vec<Json>,
    changed_spec_cids: Vec<String>,
    unchanged_semantics: Vec<String>,
}

fn compute_diff(
    from_props: &BTreeMap<String, String>,
    to_props: &BTreeMap<String, String>,
    changed_specs: &BTreeMap<String, PathBuf>,
    change_class: &str,
) -> Result<CatalogDiff, String> {
    let from_keys: BTreeSet<String> = from_props.keys().cloned().collect();
    let to_keys: BTreeSet<String> = to_props.keys().cloned().collect();
    let mut changed_spec_cids = Vec::new();

    let mut added = Vec::new();
    for key in to_keys.difference(&from_keys) {
        let to_cid = to_props.get(key).expect("present");
        validate_changed_spec(key, to_cid, changed_specs)?;
        changed_spec_cids.push(to_cid.clone());
        added.push(json!({
            "propertyKey": key,
            "toCid": to_cid,
            "layer": layer_for(key, change_class),
            "reasonCid": to_cid
        }));
    }

    let mut modified = Vec::new();
    for key in to_keys.intersection(&from_keys) {
        let from_cid = from_props.get(key).expect("present");
        let to_cid = to_props.get(key).expect("present");
        if from_cid == to_cid {
            continue;
        }
        validate_changed_spec(key, to_cid, changed_specs)?;
        changed_spec_cids.push(to_cid.clone());
        modified.push(json!({
            "propertyKey": key,
            "fromCid": from_cid,
            "toCid": to_cid,
            "layer": layer_for(key, change_class),
            "reasonCid": to_cid
        }));
    }

    let mut removed = Vec::new();
    for key in from_keys.difference(&to_keys) {
        removed.push(json!({
            "propertyKey": key,
            "fromCid": from_props.get(key).expect("present")
        }));
    }

    for key in changed_specs.keys() {
        if !to_props.contains_key(key) {
            return Err(format!(
                "changed spec `{key}` was provided but is not present in successor catalog"
            ));
        }
    }

    Ok(CatalogDiff {
        added,
        modified,
        removed,
        changed_spec_cids,
        unchanged_semantics: vec![
            "core-substrate-verification".into(),
            "proofir-grammar".into(),
            "canonicalization".into(),
            "proof-file-format".into(),
            "cross-kit-conformance-fixtures".into(),
            "language-lifter-output-semantics".into(),
        ],
    })
}

fn validate_changed_spec(
    key: &str,
    expected_cid: &str,
    changed_specs: &BTreeMap<String, PathBuf>,
) -> Result<(), String> {
    let path = changed_specs.get(key).ok_or_else(|| {
        format!("catalog property `{key}` changed; pass --changed-spec {key}=<path>")
    })?;
    let bytes = fs::read(path).map_err(|e| format!("read changed spec {}: {e}", path.display()))?;
    let actual = blake3_512_of(&bytes);
    if actual != expected_cid {
        return Err(format!(
            "changed spec `{key}` CID mismatch: catalog has {expected_cid}, file {} hashes to {actual}",
            path.display()
        ));
    }
    Ok(())
}

fn layer_for(key: &str, _change_class: &str) -> &'static str {
    if is_core_substrate_property(key) {
        "core"
    } else if key.ends_with("-protocol") {
        "extension"
    } else {
        "extension"
    }
}

fn collect_witness_inputs(body: &Json, body_cid: &str) -> Vec<String> {
    let mut inputs = vec![body_cid.to_string()];
    if let Some(body_inputs) = body.get("inputCids").and_then(|v| v.as_array()) {
        for cid in body_inputs {
            if let Some(cid) = cid.as_str() {
                inputs.push(cid.to_string());
            }
        }
    }
    inputs.sort();
    inputs.dedup();
    inputs
}

fn parse_changed_specs(bindings: &[String]) -> Result<BTreeMap<String, PathBuf>, String> {
    let mut out = BTreeMap::new();
    for binding in bindings {
        let (key, path) = binding.split_once('=').ok_or_else(|| {
            format!("invalid --changed-spec `{binding}`; expected property-key=path")
        })?;
        if key.is_empty() || path.is_empty() {
            return Err(format!(
                "invalid --changed-spec `{binding}`; property key and path must be non-empty"
            ));
        }
        out.insert(key.to_string(), PathBuf::from(path));
    }
    Ok(out)
}

fn catalog_properties(catalog: &Json) -> Result<BTreeMap<String, String>, String> {
    let props = catalog
        .get("properties")
        .and_then(|v| v.as_object())
        .ok_or_else(|| "catalog missing object field `properties`".to_string())?;
    let mut out = BTreeMap::new();
    for (key, value) in props {
        let cid = value
            .as_str()
            .ok_or_else(|| format!("catalog property `{key}` is not a string CID"))?;
        out.insert(key.clone(), cid.to_string());
    }
    Ok(out)
}

fn catalog_string(catalog: &Json, field: &str) -> Result<String, String> {
    catalog
        .get(field)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("catalog missing string field `{field}`"))
}

fn read_json(path: &Path) -> Result<Json, String> {
    let bytes = fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("parse {}: {e}", path.display()))
}

fn read_json_bytes(path: &Path) -> Result<(Json, Vec<u8>), String> {
    let bytes = fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let json =
        serde_json::from_slice(&bytes).map_err(|e| format!("parse {}: {e}", path.display()))?;
    Ok((json, bytes))
}

fn write_json(path: &Path, value: &Json) -> Result<(), String> {
    let mut out =
        serde_json::to_string_pretty(value).map_err(|e| format!("serialize JSON: {e}"))?;
    out.push('\n');
    fs::write(path, out).map_err(|e| format!("write {}: {e}", path.display()))
}

fn write_text(path: &Path, text: &str) -> Result<(), String> {
    fs::write(path, text).map_err(|e| format!("write {}: {e}", path.display()))
}

fn jcs_cid(value: &Json) -> Result<String, String> {
    let canonical = json_to_cvalue(value)?;
    let jcs = encode_jcs(&canonical);
    Ok(blake3_512_of(jcs.as_bytes()))
}

fn json_to_cvalue(value: &Json) -> Result<Arc<CValue>, String> {
    Ok(match value {
        Json::Null => CValue::null(),
        Json::Bool(b) => CValue::boolean(*b),
        Json::Number(n) => {
            let i = n
                .as_i64()
                .ok_or_else(|| format!("non-i64 JSON number cannot be canonicalized: {n}"))?;
            CValue::integer(i)
        }
        Json::String(s) => CValue::string(s.clone()),
        Json::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(json_to_cvalue(item)?);
            }
            CValue::array(out)
        }
        Json::Object(map) => {
            let mut entries = Vec::with_capacity(map.len());
            for (key, item) in map {
                entries.push((key.clone(), json_to_cvalue(item)?));
            }
            CValue::object(entries)
        }
    })
}

fn admit_protocol_evolution(
    from: &Json,
    to: &Json,
    policy: &Json,
    verifier: &Json,
    catalog_attestation_bytes: Option<&[u8]>,
    catalog_diff: &Json,
    body: &Json,
) -> Result<(), String> {
    require_json_string(body, "kind", "PEP body")?
        .eq("ProtocolEvolutionBodyClaim")
        .then_some(())
        .ok_or_else(|| "PEP body kind must be `ProtocolEvolutionBodyClaim`".to_string())?;
    require_json_string(body, "schemaVersion", "PEP body")?
        .eq("1")
        .then_some(())
        .ok_or_else(|| "PEP body schemaVersion must be `1`".to_string())?;
    require_json_string(policy, "kind", "PEP policy")?
        .eq("ProtocolEvolutionPolicy")
        .then_some(())
        .ok_or_else(|| "PEP policy kind must be `ProtocolEvolutionPolicy`".to_string())?;
    require_json_string(policy, "schemaVersion", "PEP policy")?
        .eq("1")
        .then_some(())
        .ok_or_else(|| "PEP policy schemaVersion must be `1`".to_string())?;
    require_json_string(verifier, "kind", "PEP verifier")?
        .eq("ProtocolEvolutionVerifier")
        .then_some(())
        .ok_or_else(|| "PEP verifier kind must be `ProtocolEvolutionVerifier`".to_string())?;
    require_json_string(verifier, "schemaVersion", "PEP verifier")?
        .eq("1")
        .then_some(())
        .ok_or_else(|| "PEP verifier schemaVersion must be `1`".to_string())?;

    let from_catalog_cid = jcs_cid(from)?;
    let to_catalog_cid = jcs_cid(to)?;
    if from_catalog_cid == to_catalog_cid {
        return Err("PEP admission refused: fromCatalogCid equals toCatalogCid".into());
    }
    require_body_cid(body, "fromCatalogCid", &from_catalog_cid)?;
    require_body_cid(body, "toCatalogCid", &to_catalog_cid)?;
    require_body_cid(body, "policyCid", &jcs_cid(policy)?)?;
    require_body_cid(body, "verifierCid", &jcs_cid(verifier)?)?;

    let change_class = require_json_string(body, "changeClass", "PEP body")?;
    if !matches!(
        change_class,
        "extension-only"
            | "compatible"
            | "migration-required"
            | "breaking"
            | "core-candidate"
            | "key-rotation"
    ) {
        return Err(format!(
            "PEP admission refused: unsupported changeClass `{change_class}`"
        ));
    }
    if let Some(accepted) = policy.get("acceptedChangeClass").and_then(|v| v.as_str()) {
        if accepted != change_class {
            return Err(format!(
                "PEP admission refused: policy accepts `{accepted}`, body declares `{change_class}`"
            ));
        }
    }

    require_catalog_diff_matches_body(catalog_diff, body)?;
    require_version_label_policy(body)?;
    require_change_class_compatible(body, change_class)?;
    require_input_cids_close(body)?;
    require_catalog_attestation_if_policy_names_signer(
        policy,
        catalog_attestation_bytes,
        &to_catalog_cid,
    )?;

    Ok(())
}

fn require_json_string<'a>(value: &'a Json, field: &str, context: &str) -> Result<&'a str, String> {
    value
        .get(field)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("{context} missing string field `{field}`"))
}

fn require_body_cid(body: &Json, field: &str, expected: &str) -> Result<(), String> {
    let actual = require_json_string(body, field, "PEP body")?;
    if !is_blake3_512_cid(actual) {
        return Err(format!(
            "PEP admission refused: `{field}` is not a blake3-512 CID"
        ));
    }
    if actual != expected {
        return Err(format!(
            "PEP admission refused: `{field}` is `{actual}`, expected `{expected}`"
        ));
    }
    Ok(())
}

fn require_catalog_diff_matches_body(catalog_diff: &Json, body: &Json) -> Result<(), String> {
    require_json_string(catalog_diff, "kind", "catalog diff")?
        .eq("ProtocolCatalogDiff")
        .then_some(())
        .ok_or_else(|| "catalog diff kind must be `ProtocolCatalogDiff`".to_string())?;
    require_json_string(catalog_diff, "schemaVersion", "catalog diff")?
        .eq("1")
        .then_some(())
        .ok_or_else(|| "catalog diff schemaVersion must be `1`".to_string())?;
    for field in [
        "protocolName",
        "fromCatalogCid",
        "toCatalogCid",
        "fromVersionLabel",
        "toVersionLabel",
        "changeClass",
    ] {
        let body_value = require_json_string(body, field, "PEP body")?;
        let diff_value = require_json_string(catalog_diff, field, "catalog diff")?;
        if body_value != diff_value {
            return Err(format!(
                "PEP admission refused: catalog diff `{field}` is `{diff_value}`, body has `{body_value}`"
            ));
        }
    }
    let diff_cid = jcs_cid(catalog_diff)?;
    let evidence_diff_cid = body
        .get("evidence")
        .and_then(|v| v.get("catalogDiffCid"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| "PEP body evidence missing string field `catalogDiffCid`".to_string())?;
    if evidence_diff_cid != diff_cid {
        return Err(format!(
            "PEP admission refused: evidence catalogDiffCid is `{evidence_diff_cid}`, supplied catalog diff hashes to `{diff_cid}`"
        ));
    }

    let change_set = body
        .get("changeSet")
        .and_then(|v| v.as_object())
        .ok_or_else(|| "PEP body missing object field `changeSet`".to_string())?;
    for field in ["added", "modified", "removed", "deprecated"] {
        let body_value = change_set
            .get(field)
            .ok_or_else(|| format!("PEP body changeSet missing `{field}`"))?;
        let diff_value = catalog_diff
            .get(field)
            .ok_or_else(|| format!("catalog diff missing `{field}`"))?;
        if body_value != diff_value {
            return Err(format!(
                "PEP admission refused: computed catalog diff does not match changeSet `{field}`"
            ));
        }
    }
    Ok(())
}

fn require_input_cids_close(body: &Json) -> Result<(), String> {
    let inputs = body
        .get("inputCids")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "PEP body missing array field `inputCids`".to_string())?;
    let input_set: BTreeSet<&str> = inputs
        .iter()
        .map(|v| {
            v.as_str()
                .ok_or_else(|| "PEP body inputCids must contain only strings".to_string())
        })
        .collect::<Result<_, _>>()?;

    let mut required = BTreeSet::new();
    for field in ["fromCatalogCid", "toCatalogCid", "verifierCid", "policyCid"] {
        required.insert(require_json_string(body, field, "PEP body")?);
    }
    collect_cids(body.get("changeSet").unwrap_or(&Json::Null), &mut required);
    collect_cids(body.get("evidence").unwrap_or(&Json::Null), &mut required);

    for cid in required {
        if !is_blake3_512_cid(cid) {
            return Err(format!(
                "PEP admission refused: required input `{cid}` is not a blake3-512 CID"
            ));
        }
        if !input_set.contains(cid) {
            return Err(format!(
                "PEP admission refused: inputCids missing required CID `{cid}`"
            ));
        }
    }
    Ok(())
}

fn collect_cids<'a>(value: &'a Json, out: &mut BTreeSet<&'a str>) {
    match value {
        Json::String(s) if is_blake3_512_cid(s) => {
            out.insert(s.as_str());
        }
        Json::Array(items) => {
            for item in items {
                collect_cids(item, out);
            }
        }
        Json::Object(map) => {
            for item in map.values() {
                collect_cids(item, out);
            }
        }
        _ => {}
    }
}

fn collect_owned_cids(value: &Json, out: &mut BTreeSet<String>) {
    match value {
        Json::String(s) if is_blake3_512_cid(s) => {
            out.insert(s.clone());
        }
        Json::Array(items) => {
            for item in items {
                collect_owned_cids(item, out);
            }
        }
        Json::Object(map) => {
            for item in map.values() {
                collect_owned_cids(item, out);
            }
        }
        _ => {}
    }
}

fn require_version_label_policy(body: &Json) -> Result<(), String> {
    let change_class = require_json_string(body, "changeClass", "PEP body")?;
    if change_class != "extension-only" {
        return Ok(());
    }
    let from = parse_semver_prefix(require_json_string(body, "fromVersionLabel", "PEP body")?)
        .ok_or_else(|| {
            "PEP admission refused: fromVersionLabel is not semver-shaped".to_string()
        })?;
    let to = parse_semver_prefix(require_json_string(body, "toVersionLabel", "PEP body")?)
        .ok_or_else(|| "PEP admission refused: toVersionLabel is not semver-shaped".to_string())?;
    if from.0 != to.0 || from.1 != to.1 || to.2 <= from.2 {
        return Err(format!(
            "PEP admission refused: extension-only version label must move within the same major/minor patch line, got v{}.{}.{} -> v{}.{}.{}",
            from.0, from.1, from.2, to.0, to.1, to.2
        ));
    }
    Ok(())
}

fn parse_semver_prefix(label: &str) -> Option<(u64, u64, u64)> {
    let label = label.strip_prefix('v').unwrap_or(label);
    let core = label.split('-').next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

fn require_change_class_compatible(body: &Json, change_class: &str) -> Result<(), String> {
    if change_class != "extension-only" {
        return Ok(());
    }
    let change_set = body
        .get("changeSet")
        .and_then(|v| v.as_object())
        .ok_or_else(|| "PEP body missing object field `changeSet`".to_string())?;
    for field in ["added", "modified", "removed"] {
        let entries = change_set
            .get(field)
            .and_then(|v| v.as_array())
            .ok_or_else(|| format!("PEP body changeSet missing array `{field}`"))?;
        for entry in entries {
            let key = entry
                .get("propertyKey")
                .and_then(|v| v.as_str())
                .ok_or_else(|| format!("PEP body changeSet `{field}` entry missing propertyKey"))?;
            if is_core_substrate_property(key) {
                return Err(format!(
                    "PEP admission refused: extension-only cannot change core substrate property `{key}`"
                ));
            }
            if let Some(layer) = entry.get("layer").and_then(|v| v.as_str()) {
                let expected = layer_for(key, change_class);
                if layer != expected {
                    return Err(format!(
                        "PEP admission refused: property `{key}` declares layer `{layer}`, expected `{expected}`"
                    ));
                }
            }
        }
    }
    Ok(())
}

fn require_catalog_attestation_if_policy_names_signer(
    policy: &Json,
    attestation_bytes: Option<&[u8]>,
    expected_catalog_cid: &str,
) -> Result<(), String> {
    let Some(signer) = policy.get("acceptedSigner").and_then(|v| v.as_str()) else {
        return Ok(());
    };
    let Some(bytes) = attestation_bytes else {
        return Err(
            "PEP admission refused: policy names acceptedSigner but no catalog attestation was provided"
                .into(),
        );
    };
    let verdict = verify_signed_attestation(bytes, signer, expected_catalog_cid)
        .map_err(|e| format!("PEP admission refused: catalog attestation malformed: {e:#}"))?;
    if !verdict.ok() {
        return Err(format!(
            "PEP admission refused: catalog attestation failed (cidMatches={}, signerMatches={}, signatureOk={})",
            verdict.cid_matches, verdict.signer_matches, verdict.signature_ok
        ));
    }
    Ok(())
}

fn is_core_substrate_property(key: &str) -> bool {
    matches!(
        key,
        "ir-formal-grammar"
            | "canonicalization-grammar"
            | "memento-envelope-grammar"
            | "signatures-and-non-repudiation"
            | "chain-validity-and-fail-closed"
            | "proof-file-format"
            | "semantic-envelope"
            | "protocol-catalog-format"
            | "substrate-layers-envelope-header-body"
            | "version-chains-pinning"
    )
}

fn is_blake3_512_cid(s: &str) -> bool {
    let Some(hex) = s.strip_prefix("blake3-512:") else {
        return false;
    };
    hex.len() == 128 && hex.chars().all(|c| c.is_ascii_hexdigit())
}
