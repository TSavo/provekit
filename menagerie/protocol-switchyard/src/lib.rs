// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Parser;
use provekit_canonicalizer::blake3_512_of;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const EXIT_OK: u8 = 0;
const EXIT_VERIFY_FAIL: u8 = 1;
const EXIT_USER_ERROR: u8 = 2;

const PROTOCOL_NAME: &str = "protocol-switchyard-http";
const PROFILE_FROM_VERSION: &str = "v1.0.0";
const PROFILE_TO_VERSION: &str = "v1.0.1";
const CHANGE_CLASS: &str = "extension-only";

#[derive(Parser, Debug, Clone, Default)]
pub struct OutputFlags {
    #[arg(long)]
    pub json: bool,
    #[arg(long)]
    pub quiet: bool,
}

#[derive(Parser, Debug, Clone)]
#[command(
    name = "provekit-protocol-switchyard",
    version,
    about = "Run Protocol Switchyard exhibits: protocol versions as roots, migrations as witnessed edges.",
    long_about = "Protocol Switchyard cashes out paper 10's claim. It builds two HTTP profile catalog roots \
(v1 and v2), each naming spec-document property CIDs, then shells through the provekit CLI to mint a \
witnessed migration edge between them. Every evidentiary CID emitted is a blake3-512 the runner can \
recompute from source bytes."
)]
pub struct SwitchyardArgs {
    /// Run all exhibits.
    #[arg(long)]
    pub all: bool,
    #[command(flatten)]
    pub out: OutputFlags,
}

#[derive(Debug)]
enum SwitchyardError {
    Setup(String),
    Verify(String),
}

pub fn run(args: SwitchyardArgs) -> u8 {
    if !args.all {
        eprintln!("protocol-switchyard: pass --all to run all exhibits");
        return EXIT_USER_ERROR;
    }
    let repo_root = match find_repo_root() {
        Ok(root) => root,
        Err(e) => {
            eprintln!("protocol-switchyard: {e}");
            return EXIT_USER_ERROR;
        }
    };

    let exhibit_dir = repo_root.join("menagerie/protocol-switchyard");

    match run_http_profile_migration(&repo_root, &exhibit_dir) {
        Ok(report) => {
            if args.out.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report).expect("report serializes")
                );
            } else if !args.out.quiet {
                print_human(&report);
            }
            EXIT_OK
        }
        Err(SwitchyardError::Setup(e)) => {
            eprintln!("protocol-switchyard: setup error: {e}");
            EXIT_USER_ERROR
        }
        Err(SwitchyardError::Verify(e)) => {
            eprintln!("protocol-switchyard: verification failed: {e}");
            EXIT_VERIFY_FAIL
        }
    }
}

#[derive(Debug, Serialize)]
struct ExhibitReport {
    exhibit: &'static str,
    claim: &'static str,
    protocol_name: &'static str,
    from_version: &'static str,
    to_version: &'static str,
    change_class: &'static str,
    spec_cids: Vec<SpecCidEntry>,
    from_catalog_cid: String,
    to_catalog_cid: String,
    body_cid: String,
    witness_cid: String,
    out_dir: String,
    aspirational: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct SpecCidEntry {
    profile: &'static str,
    property_key: &'static str,
    spec_path: String,
    cid: String,
}

#[derive(Debug, Deserialize)]
struct EvolveJson {
    #[serde(rename = "fromCatalogCid")]
    from_catalog_cid: String,
    #[serde(rename = "toCatalogCid")]
    to_catalog_cid: String,
    #[serde(rename = "bodyCid")]
    body_cid: String,
    #[serde(rename = "witnessCid")]
    witness_cid: String,
    #[serde(rename = "outDir")]
    out_dir: String,
}

struct SpecBinding {
    profile: &'static str,
    property_key: &'static str,
    spec_path: PathBuf,
}

fn run_http_profile_migration(
    repo_root: &Path,
    exhibit_dir: &Path,
) -> Result<ExhibitReport, SwitchyardError> {
    let v1_smuggling = exhibit_dir.join("profiles/http/v1/specs/request-smuggling-refusal.md");
    let v1_framing = exhibit_dir.join("profiles/http/v1/specs/content-length-transfer-encoding.md");
    let v2_smuggling = exhibit_dir.join("profiles/http/v2/specs/request-smuggling-refusal.md");
    let v2_framing = exhibit_dir.join("profiles/http/v2/specs/content-length-transfer-encoding.md");

    let v1_bindings = vec![
        SpecBinding {
            profile: "v1",
            property_key: "request-smuggling-refusal",
            spec_path: v1_smuggling.clone(),
        },
        SpecBinding {
            profile: "v1",
            property_key: "content-length-transfer-encoding",
            spec_path: v1_framing.clone(),
        },
    ];
    let v2_bindings = vec![
        SpecBinding {
            profile: "v2",
            property_key: "request-smuggling-refusal",
            spec_path: v2_smuggling.clone(),
        },
        SpecBinding {
            profile: "v2",
            property_key: "content-length-transfer-encoding",
            spec_path: v2_framing.clone(),
        },
    ];

    let v1_props = build_property_map(&v1_bindings)?;
    let v2_props = build_property_map(&v2_bindings)?;

    if v1_props == v2_props {
        return Err(SwitchyardError::Setup(
            "v1 and v2 spec property CIDs are identical; nothing to evolve".into(),
        ));
    }

    let from_catalog = build_catalog(PROFILE_FROM_VERSION, &v1_props);
    let to_catalog = build_catalog(PROFILE_TO_VERSION, &v2_props);

    let work_dir = temp_work_dir("http-migration").map_err(SwitchyardError::Setup)?;
    fs::create_dir_all(&work_dir)
        .map_err(|e| SwitchyardError::Setup(format!("mkdir {}: {e}", work_dir.display())))?;
    let from_path = work_dir.join("from-catalog.json");
    let to_path = work_dir.join("to-catalog.json");
    let policy_path = work_dir.join("policy.json");
    let verifier_path = work_dir.join("verifier.json");
    let out_dir = work_dir.join("witness");

    write_json(&from_path, &from_catalog).map_err(SwitchyardError::Setup)?;
    write_json(&to_path, &to_catalog).map_err(SwitchyardError::Setup)?;
    write_json(&policy_path, &build_policy()).map_err(SwitchyardError::Setup)?;
    write_json(&verifier_path, &build_verifier()).map_err(SwitchyardError::Setup)?;

    let mut cmd = provekit_cli_command(repo_root)?;
    cmd.arg("protocol")
        .arg("evolve")
        .arg("--from")
        .arg(&from_path)
        .arg("--to")
        .arg(&to_path)
        .arg("--policy")
        .arg(&policy_path)
        .arg("--verifier")
        .arg(&verifier_path)
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--change-class")
        .arg(CHANGE_CLASS)
        .arg("--producer")
        .arg("provekit-protocol-switchyard")
        .arg("--json")
        .arg("--quiet")
        .current_dir(repo_root)
        .stdin(Stdio::null());

    for binding in v2_bindings.iter() {
        cmd.arg("--changed-spec").arg(format!(
            "{}={}",
            binding.property_key,
            binding.spec_path.display()
        ));
    }

    let output = cmd
        .output()
        .map_err(|e| SwitchyardError::Setup(format!("spawn provekit protocol evolve: {e}")))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(SwitchyardError::Verify(format!(
            "provekit protocol evolve failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
        )));
    }

    let parsed: EvolveJson = serde_json::from_str(&stdout).map_err(|e| {
        SwitchyardError::Verify(format!(
            "parse provekit protocol evolve JSON: {e}\nstdout:\n{stdout}"
        ))
    })?;

    let mut spec_cids = Vec::new();
    for binding in v1_bindings.iter().chain(v2_bindings.iter()) {
        let cid = v1_or_v2_cid(binding, &v1_props, &v2_props)?;
        spec_cids.push(SpecCidEntry {
            profile: binding.profile,
            property_key: binding.property_key,
            spec_path: binding
                .spec_path
                .strip_prefix(repo_root)
                .unwrap_or(&binding.spec_path)
                .display()
                .to_string(),
            cid,
        });
    }

    Ok(ExhibitReport {
        exhibit: "http-v1-to-v2",
        claim: "two HTTP profile roots, one witnessed migration edge between them",
        protocol_name: PROTOCOL_NAME,
        from_version: PROFILE_FROM_VERSION,
        to_version: PROFILE_TO_VERSION,
        change_class: CHANGE_CLASS,
        spec_cids,
        from_catalog_cid: parsed.from_catalog_cid,
        to_catalog_cid: parsed.to_catalog_cid,
        body_cid: parsed.body_cid,
        witness_cid: parsed.witness_cid,
        out_dir: parsed.out_dir,
        aspirational: aspirational_notes(),
    })
}

fn aspirational_notes() -> Vec<&'static str> {
    vec![
        "spec files are prose obligations; the runner does not execute grammar conformance against them",
        "no implementation conformance witness is minted; only the migration edge between profile roots is witnessed",
        "the v1->v2 transition is encoded as PEP `extension-only` over a synthetic profile catalog, not as a paper-10-section-6 migration body with bridge checker witnesses",
    ]
}

fn v1_or_v2_cid(
    binding: &SpecBinding,
    v1_props: &std::collections::BTreeMap<String, String>,
    v2_props: &std::collections::BTreeMap<String, String>,
) -> Result<String, SwitchyardError> {
    let map = if binding.profile == "v1" {
        v1_props
    } else {
        v2_props
    };
    map.get(binding.property_key).cloned().ok_or_else(|| {
        SwitchyardError::Verify(format!(
            "spec binding {} {} missing from property map",
            binding.profile, binding.property_key
        ))
    })
}

fn build_property_map(
    bindings: &[SpecBinding],
) -> Result<std::collections::BTreeMap<String, String>, SwitchyardError> {
    let mut out = std::collections::BTreeMap::new();
    for binding in bindings {
        let bytes = fs::read(&binding.spec_path).map_err(|e| {
            SwitchyardError::Setup(format!("read {}: {e}", binding.spec_path.display()))
        })?;
        out.insert(binding.property_key.to_string(), blake3_512_of(&bytes));
    }
    Ok(out)
}

fn build_catalog(version: &str, properties: &std::collections::BTreeMap<String, String>) -> Value {
    json!({
        "kind": "catalog",
        "name": PROTOCOL_NAME,
        "version": version,
        "algorithms": {
            "hash": ["blake3-512"],
            "signature": ["ed25519"],
            "pubkey": ["ed25519"]
        },
        "properties": properties,
        "declaredAt": "2026-05-09T00:00:00Z"
    })
}

fn build_policy() -> Value {
    json!({
        "kind": "ProtocolEvolutionPolicy",
        "schemaVersion": "1",
        "name": "protocol-switchyard-http-v1-to-v2",
        "acceptedChangeClass": CHANGE_CLASS,
        "versionLabelRule": {
            "extensionOnlyWithoutCrossKitSemanticObligation": "patch"
        },
        "requiredChecks": [
            "from catalog CID recomputes from from-catalog.json",
            "to catalog CID recomputes from to-catalog.json",
            "modified properties are content-addressed by spec file bytes",
            "no core substrate property is changed",
            "change class is extension-only"
        ],
        "nonGoals": [
            "execute grammar conformance against the prose specs",
            "mint an implementation conformance witness for any HTTP server",
            "claim that v2 conformance implies bug-for-bug compatibility with any deployed HTTP stack"
        ]
    })
}

fn build_verifier() -> Value {
    json!({
        "kind": "ProtocolEvolutionVerifier",
        "schemaVersion": "1",
        "name": "protocol-switchyard-http-verifier",
        "version": "0.1.0",
        "acceptedTools": [
            {
                "name": "blake3-512 of spec bytes",
                "command": "cargo run --manifest-path menagerie/protocol-switchyard/Cargo.toml -- --all",
                "purpose": "Recompute property CIDs by hashing the spec files in this exhibit."
            }
        ],
        "acceptedManualAssertions": [
            "v2 strengthens the v1 framing-ambiguity refusal by closing additional boundary cases.",
            "v1 conformance witnesses do not imply v2 conformance.",
            "this exhibit demonstrates the witnessed-edge shape, not implementation conformance."
        ]
    })
}

fn print_human(report: &ExhibitReport) {
    println!("ProvekIt Protocol Switchyard");
    println!("  exhibit       : {}", report.exhibit);
    println!("  claim         : {}", report.claim);
    println!("  protocol      : {}", report.protocol_name);
    println!("  from version  : {}", report.from_version);
    println!("  to version    : {}", report.to_version);
    println!("  change class  : {}", report.change_class);
    println!("  spec CIDs:");
    for entry in &report.spec_cids {
        println!(
            "    [{}] {:38} = {}",
            entry.profile, entry.property_key, entry.cid
        );
    }
    println!("  fromCatalogCid: {}", report.from_catalog_cid);
    println!("  toCatalogCid  : {}", report.to_catalog_cid);
    println!("  bodyCid       : {}", report.body_cid);
    println!("  witnessCid    : {}", report.witness_cid);
    println!("  out dir       : {}", report.out_dir);
    println!("  aspirational:");
    for note in &report.aspirational {
        println!("    - {note}");
    }
    println!("protocol-switchyard: http-v1-to-v2 PASS");
}

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    let mut text =
        serde_json::to_string_pretty(value).map_err(|e| format!("serialize JSON: {e}"))?;
    text.push('\n');
    fs::write(path, text).map_err(|e| format!("write {}: {e}", path.display()))
}

fn find_repo_root() -> Result<PathBuf, String> {
    let mut cursor = std::env::current_dir().map_err(|e| format!("current dir: {e}"))?;
    cursor = cursor.canonicalize().unwrap_or(cursor);
    loop {
        if cursor
            .join("implementations/rust/provekit-cli/Cargo.toml")
            .exists()
        {
            return Ok(cursor);
        }
        if !cursor.pop() {
            return Err("could not find repo root from current directory".into());
        }
    }
}

fn temp_work_dir(label: &str) -> Result<PathBuf, String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("system clock before UNIX_EPOCH: {e}"))?
        .as_nanos();
    Ok(std::env::temp_dir().join(format!(
        "provekit-protocol-switchyard-{label}-{}-{now}",
        std::process::id()
    )))
}

fn provekit_cli_command(repo_root: &Path) -> Result<Command, SwitchyardError> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let manifest = repo_root.join("implementations/rust/provekit-cli/Cargo.toml");
    let mut cmd = Command::new(cargo);
    cmd.arg("run")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(manifest)
        .arg("--");
    Ok(cmd)
}
