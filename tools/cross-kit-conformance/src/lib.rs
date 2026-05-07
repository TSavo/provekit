use provekit_canonicalizer::{blake3_512_of, encode_jcs, Value as CValue};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use std::collections::{HashSet, VecDeque};
use std::ffi::OsString;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

type Result<T> = std::result::Result<T, String>;

const EXPECTED_CATALOG_VERSION: &str = "v1.6.2-2026-05-07";
const EXPECTED_CATALOG_CID: &str = concat!(
    "blake3-512:",
    "52bdb2be4b381cec2aff95db7755c84184878b45cd91882d262114a1abd2dd51",
    "3f9ef3b250fb87093316fd0fcb48e4b97e109d463e57df5bda6aac0b1c719a0f"
);
const EXPECTED_PROTOCOL_CONTRACT_SET_CID: &str = concat!(
    "blake3-512:",
    "2a4dc95d1af1ff9f7f5a3414dd7fef67ab342155f4ff204aaf333b2dab6441ec",
    "ddd2ed2d53aaabb5c929eefa8d4155a9f7a1725f8ea2febefe04c4f7365c27ab"
);
const EMPTY_CONTRACT_SET_CID: &str = concat!(
    "blake3-512:",
    "d53d18c23212ea7b6300594bb89bce60218f6eff2b9d628b8cc42d3e79bbd5ab",
    "09994845815cc7185113418f9fc2edc7606b06f0d57a6d581e7cff5b290f3229"
);

const CORE_FIXTURES: &[&str] = &[
    "eq_atomic",
    "pattern1_bounded_loop",
    "contract_decl",
    "bridge_decl_v1_1",
];

const RUST_CORE_FIXTURES: &[&str] = &["eq_atomic", "pattern1_bounded_loop", "contract_decl"];

const ALL_KITS: &[&str] = &[
    "rust",
    "go",
    "cpp",
    "typescript",
    "csharp",
    "java",
    "python",
    "c",
    "zig",
    "php",
    "swift",
    "ruby",
];

#[derive(Debug, Clone, Deserialize)]
pub struct Fixture {
    pub name: String,
    pub capability: String,
    pub description: String,
    pub jcs: String,
    pub hash: String,
}

#[derive(Debug, Deserialize)]
struct FixtureToml {
    catalog_version: String,
    catalog_cid: String,
    fixture: Vec<Fixture>,
}

#[derive(Debug, Clone)]
struct KitSelfContractAttestation {
    kit: String,
    attestation_lang: String,
    contract_set_cid: String,
    path: PathBuf,
}

#[derive(Debug, Deserialize)]
struct SelfContractAttestationJson {
    #[serde(rename = "schemaVersion")]
    schema_version: String,
    kind: String,
    lang: String,
    cid: String,
    #[serde(rename = "contractSetCid")]
    contract_set_cid: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RunConfig {
    profile: Profile,
    jobs: usize,
    bootstrap_self_contract_attestations: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Profile {
    Linux,
    Swift,
    All,
}

impl Profile {
    fn parse(s: &str) -> Result<Self> {
        match s {
            "linux" => Ok(Self::Linux),
            "swift" => Ok(Self::Swift),
            "all" => Ok(Self::All),
            _ => Err(format!(
                "unknown profile `{s}`; expected linux, swift, or all"
            )),
        }
    }

    fn default_for_host() -> Self {
        if cfg!(target_os = "macos") {
            Self::All
        } else {
            Self::Linux
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Linux => "linux",
            Self::Swift => "swift",
            Self::All => "all",
        }
    }

    fn required_kits(self) -> Vec<&'static str> {
        match self {
            Self::Linux => ALL_KITS
                .iter()
                .copied()
                .filter(|kit| *kit != "swift")
                .collect(),
            Self::Swift => vec!["swift"],
            Self::All => ALL_KITS.to_vec(),
        }
    }
}

#[derive(Debug)]
struct ProcResult {
    code: i32,
    stdout: String,
    stderr: String,
}

#[derive(Debug)]
struct DirectAdapter {
    kit: &'static str,
    emit: fn(&str) -> Result<String>,
    fixtures: &'static [&'static str],
}

#[derive(Debug)]
struct NativeCheck {
    kit: &'static str,
    name: &'static str,
    cmd: Vec<String>,
    cwd: PathBuf,
    timeout: Duration,
}

#[derive(Debug)]
enum DirectCheckOutcome {
    Pass,
    MissingFixture(String),
    AdapterError(String),
    MalformedCid(String),
    CidMismatch { got: String, want: String },
}

#[derive(Debug)]
struct DirectCheckResult {
    kit: &'static str,
    fixture_name: String,
    capability: String,
    outcome: DirectCheckOutcome,
}

#[derive(Debug)]
struct NativeCheckResult {
    name: &'static str,
    cmd: Vec<String>,
    proc: ProcResult,
}

type OrderedJob<T> = Box<dyn FnOnce() -> T + Send + 'static>;

#[derive(Debug, Clone, Copy)]
struct SelfContractProducer {
    kit: &'static str,
    attestation_lang: &'static str,
    mint_kit_alias: &'static str,
}

#[derive(Debug)]
struct MintStdoutArtifact {
    bundle_cid: String,
    contract_set_cid: String,
}

#[derive(Debug)]
struct LiveSelfContractArtifact {
    kit: &'static str,
    attestation_lang: &'static str,
    bundle_cid: String,
    contract_set_cid: String,
    proof_bytes_len: usize,
    proof_member_count: usize,
    proof_contract_count: usize,
}

#[derive(Debug)]
struct LiveProofArtifactValidation {
    proof_bytes_len: usize,
    proof_member_count: usize,
    proof_contract_count: usize,
    derived_contract_set_cid: String,
}

#[derive(Debug)]
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Result<Self> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| format!("system time before UNIX_EPOCH: {e}"))?
            .as_nanos();
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("{prefix}_{}_{}_{}", std::process::id(), nanos, id));
        fs::create_dir_all(&path).map_err(|e| format!("create {}: {e}", path.display()))?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("tool dir has parent")
        .parent()
        .expect("tools dir has parent")
        .to_path_buf()
}

fn fixtures_toml() -> PathBuf {
    repo_root().join("conformance/fixtures.toml")
}

fn catalog_json() -> PathBuf {
    repo_root().join("protocol/specs/2026-04-30-protocol-catalog.json")
}

fn json_lit(s: &str) -> String {
    serde_json::to_string(s).expect("string JSON serialization cannot fail")
}

fn write_file(path: &Path, body: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    fs::write(path, body).map_err(|e| format!("write {}: {e}", path.display()))
}

fn require_fixture<'a>(
    fixtures: &'a std::collections::BTreeMap<String, Fixture>,
    name: &str,
) -> Result<&'a Fixture> {
    fixtures
        .get(name)
        .ok_or_else(|| format!("missing conformance fixture `{name}`"))
}

fn run_cmd(cmd: &[String], cwd: &Path, timeout: Duration) -> ProcResult {
    run_cmd_env(cmd, cwd, timeout, &[])
}

fn run_cmd_env(
    cmd: &[String],
    cwd: &Path,
    timeout: Duration,
    envs: &[(&str, OsString)],
) -> ProcResult {
    if cmd.is_empty() {
        return ProcResult {
            code: 127,
            stdout: String::new(),
            stderr: "empty command".to_string(),
        };
    }

    let mut command = Command::new(&cmd[0]);
    command
        .args(&cmd[1..])
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if !envs.iter().any(|(key, _)| *key == "PATH") {
        let current_path = std::env::var_os("PATH");
        command.env(
            "PATH",
            prepend_unique_path_dirs(current_path.as_ref(), &runtime_bin_dir_candidates()),
        );
    }
    if !envs.iter().any(|(key, _)| *key == "JAVA_HOME") {
        if let Some(java_home) = detect_java_home() {
            command.env("JAVA_HOME", java_home);
        }
    }
    for (key, value) in envs {
        command.env(key, value);
    }

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            return ProcResult {
                code: 127,
                stdout: String::new(),
                stderr: e.to_string(),
            }
        }
    };

    let stdout_reader = child.stdout.take().map(spawn_output_reader);
    let stderr_reader = child.stderr.take().map(spawn_output_reader);
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return ProcResult {
                    code: status.code().unwrap_or(1),
                    stdout: String::from_utf8_lossy(&join_output_reader(stdout_reader)).to_string(),
                    stderr: String::from_utf8_lossy(&join_output_reader(stderr_reader)).to_string(),
                };
            }
            Ok(None) => {
                if started.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    let stdout = join_output_reader(stdout_reader);
                    let mut stderr =
                        String::from_utf8_lossy(&join_output_reader(stderr_reader)).to_string();
                    stderr.push_str(&format!("\ntimeout after {}s", timeout.as_secs()));
                    return ProcResult {
                        code: -1,
                        stdout: String::from_utf8_lossy(&stdout).to_string(),
                        stderr,
                    };
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return ProcResult {
                    code: 1,
                    stdout: String::from_utf8_lossy(&join_output_reader(stdout_reader)).to_string(),
                    stderr: format!(
                        "{}\n{}",
                        e,
                        String::from_utf8_lossy(&join_output_reader(stderr_reader))
                    ),
                };
            }
        }
    }
}

fn runtime_bin_dir_candidates() -> Vec<PathBuf> {
    [
        "/usr/local/opt/ruby/bin",
        "/opt/homebrew/opt/ruby/bin",
        "/usr/local/opt/openjdk/bin",
        "/opt/homebrew/opt/openjdk/bin",
    ]
    .into_iter()
    .map(PathBuf::from)
    .filter(|p| p.is_dir())
    .collect()
}

fn ruby_bundle_exec_cmd(args: &[&str]) -> Vec<String> {
    ["ruby", "-S", "bundle", "exec", "ruby"]
        .into_iter()
        .chain(args.iter().copied())
        .map(String::from)
        .collect()
}

fn detect_java_home() -> Option<PathBuf> {
    ["/usr/local/opt/openjdk", "/opt/homebrew/opt/openjdk"]
        .into_iter()
        .map(PathBuf::from)
        .find(|home| home.join("bin/java").is_file())
}

fn prepend_unique_path_dirs(base: Option<&OsString>, dirs: &[PathBuf]) -> OsString {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for dir in dirs {
        if seen.insert(dir.clone()) {
            out.push(dir.clone());
        }
    }
    if let Some(base) = base {
        for dir in std::env::split_paths(base) {
            if seen.insert(dir.clone()) {
                out.push(dir);
            }
        }
    }
    std::env::join_paths(out).unwrap_or_else(|_| base.cloned().unwrap_or_default())
}

fn spawn_output_reader<R>(mut pipe: R) -> thread::JoinHandle<Vec<u8>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = pipe.read_to_end(&mut bytes);
        bytes
    })
}

fn join_output_reader(handle: Option<thread::JoinHandle<Vec<u8>>>) -> Vec<u8> {
    handle.and_then(|h| h.join().ok()).unwrap_or_default()
}

fn command_stdout(cmd: &[String], cwd: &Path, timeout: Duration) -> Result<String> {
    let p = run_cmd(cmd, cwd, timeout);
    if p.code != 0 {
        return Err(command_error(cmd, &p));
    }
    Ok(p.stdout.trim().to_string())
}

fn command_error(cmd: &[String], p: &ProcResult) -> String {
    let output = format!("{}\n{}", p.stderr.trim(), p.stdout.trim());
    format!(
        "{} exited {}:\n{}",
        cmd.join(" "),
        p.code,
        tail(&output, 4000)
    )
}

fn tail(s: &str, n: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= n {
        s.to_string()
    } else {
        chars[chars.len() - n..].iter().collect()
    }
}

fn cid_is_well_formed(s: &str) -> bool {
    let Some(hex) = s.strip_prefix("blake3-512:") else {
        return false;
    };
    hex.len() == 128
        && hex
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn protocol_contract_set_cid() -> Result<String> {
    provekit_self_contracts::lift_plugin_protocol_contract_set_cid()
}

fn attestation_lang_for_kit(kit: &str) -> &str {
    match kit {
        "typescript" => "ts",
        other => other,
    }
}

fn self_contract_producer_for(kit: &'static str) -> SelfContractProducer {
    match kit {
        "typescript" => SelfContractProducer {
            kit,
            attestation_lang: "ts",
            mint_kit_alias: "ts",
        },
        other => SelfContractProducer {
            kit,
            attestation_lang: other,
            mint_kit_alias: other,
        },
    }
}

fn self_contract_producers(profile: Profile) -> Vec<SelfContractProducer> {
    profile
        .required_kits()
        .into_iter()
        .map(self_contract_producer_for)
        .collect()
}

fn self_contract_bootstrap_targets(profile: Profile) -> Vec<&'static str> {
    let mut targets = vec!["build-rust"];
    for producer in self_contract_producers(profile) {
        let target = match producer.kit {
            "go" => Some("build-go"),
            "cpp" => Some("build-cpp"),
            "typescript" => Some("build-ts"),
            "csharp" => Some("build-csharp"),
            "java" => Some("build-java-self-contracts"),
            "ruby" => Some("build-ruby"),
            "c" => Some("build-c-self-contracts"),
            "zig" => Some("build-zig"),
            "swift" => Some("build-swift"),
            _ => None,
        };
        if let Some(target) = target {
            if !targets.contains(&target) {
                targets.push(target);
            }
        }
    }
    targets
}

fn attestation_path(lang: &str) -> PathBuf {
    repo_root()
        .join(".provekit/self-contracts-attestations")
        .join(format!("{lang}.json"))
}

fn load_self_contract_attestations(profile: Profile) -> Result<Vec<KitSelfContractAttestation>> {
    let mut out = Vec::new();
    for kit in profile.required_kits() {
        let attestation_lang = attestation_lang_for_kit(kit);
        let path = attestation_path(attestation_lang);
        let raw = fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
        let parsed: SelfContractAttestationJson =
            serde_json::from_str(&raw).map_err(|e| format!("parse {}: {e}", path.display()))?;
        if parsed.schema_version != "1" {
            return Err(format!(
                "{} schemaVersion={:?}; expected \"1\"",
                path.display(),
                parsed.schema_version
            ));
        }
        if parsed.kind != "self-contracts-attestation" {
            return Err(format!(
                "{} kind={:?}; expected self-contracts-attestation",
                path.display(),
                parsed.kind
            ));
        }
        if parsed.lang != attestation_lang {
            return Err(format!(
                "{} lang={:?}; expected {:?}",
                path.display(),
                parsed.lang,
                attestation_lang
            ));
        }
        if !cid_is_well_formed(&parsed.cid) {
            return Err(format!(
                "{} cid is malformed: {}",
                path.display(),
                parsed.cid
            ));
        }
        if !cid_is_well_formed(&parsed.contract_set_cid) {
            return Err(format!(
                "{} contractSetCid is malformed: {}",
                path.display(),
                parsed.contract_set_cid
            ));
        }
        if parsed.contract_set_cid == EMPTY_CONTRACT_SET_CID {
            return Err(format!(
                "{} contractSetCid is the empty-set sentinel; self-contracts are not wired",
                path.display()
            ));
        }
        out.push(KitSelfContractAttestation {
            kit: kit.to_string(),
            attestation_lang: attestation_lang.to_string(),
            contract_set_cid: parsed.contract_set_cid,
            path,
        });
    }
    Ok(out)
}

fn parse_mint_stdout(stdout: &str) -> Result<MintStdoutArtifact> {
    let bundle_cid = stdout
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("blake3-512:"))
        .ok_or_else(|| "mint output missing live bundle CID".to_string())?
        .to_string();
    if !cid_is_well_formed(&bundle_cid) {
        return Err(format!("mint output bundle CID is malformed: {bundle_cid}"));
    }

    let contract_set_cid = stdout
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("contractSetCid:").map(str::trim))
        .ok_or_else(|| "mint output missing contractSetCid".to_string())?
        .to_string();
    if !cid_is_well_formed(&contract_set_cid) {
        return Err(format!(
            "mint output contractSetCid is malformed: {contract_set_cid}"
        ));
    }

    Ok(MintStdoutArtifact {
        bundle_cid,
        contract_set_cid,
    })
}

fn provekit_bin() -> PathBuf {
    repo_root().join("implementations/rust/target/release/provekit")
}

fn json_to_cvalue(j: &JsonValue) -> Arc<CValue> {
    match j {
        JsonValue::Null => CValue::null(),
        JsonValue::Bool(b) => CValue::boolean(*b),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                CValue::integer(i)
            } else if let Some(u) = n.as_u64() {
                CValue::integer(u as i64)
            } else {
                CValue::integer(0)
            }
        }
        JsonValue::String(s) => CValue::string(s.clone()),
        JsonValue::Array(items) => CValue::array(items.iter().map(json_to_cvalue).collect()),
        JsonValue::Object(map) => CValue::object(
            map.iter()
                .map(|(k, v)| (k.clone(), json_to_cvalue(v)))
                .collect::<Vec<_>>(),
        ),
    }
}

fn compute_contract_set_cid(mut contract_cids: Vec<String>) -> String {
    contract_cids.sort();
    let values = contract_cids
        .into_iter()
        .map(CValue::string)
        .collect::<Vec<_>>();
    let jcs = encode_jcs(&CValue::array(values));
    blake3_512_of(jcs.as_bytes())
}

fn contract_content_cid_from_memento(envelope: &JsonValue) -> Result<String> {
    let body = provekit_verifier::types::memento_body(envelope)
        .ok_or_else(|| "contract memento has no body/header".to_string())?;
    let name = body
        .get("name")
        .or_else(|| body.get("contractName"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| "contract memento missing name/contractName".to_string())?;
    let out_binding = body
        .get("outBinding")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "contract memento missing string outBinding".to_string())?;

    let mut entries = vec![
        ("name".to_string(), CValue::string(name.to_string())),
        (
            "outBinding".to_string(),
            CValue::string(out_binding.to_string()),
        ),
    ];
    for key in ["pre", "post", "inv"] {
        if let Some(value) = body.get(key) {
            entries.push((key.to_string(), json_to_cvalue(value)));
        }
    }

    let derived = blake3_512_of(encode_jcs(&CValue::object(entries)).as_bytes());
    if let Some(header_cid) = body.get("cid").and_then(|v| v.as_str()) {
        if header_cid != derived {
            return Err(format!(
                "contract content CID drift:\n  got:  {derived}\n  want: {header_cid}"
            ));
        }
    }
    Ok(derived)
}

fn contract_content_cids_from_pool(pool: &provekit_verifier::MementoPool) -> Result<Vec<String>> {
    let mut cids = Vec::new();
    for (memento_cid, envelope) in &pool.mementos {
        if provekit_verifier::types::memento_kind(envelope) == Some("contract") {
            let cid = contract_content_cid_from_memento(envelope)
                .map_err(|e| format!("contract member {memento_cid}: {e}"))?;
            cids.push(cid);
        }
    }
    Ok(cids)
}

fn validate_live_proof_artifact(
    kit: &str,
    artifact_dir: &Path,
    bundle_cid: &str,
) -> Result<LiveProofArtifactValidation> {
    let proof_path = artifact_dir.join(format!("{bundle_cid}.proof"));
    let bytes = fs::read(&proof_path).map_err(|e| format!("read {}: {e}", proof_path.display()))?;
    if bytes.is_empty() {
        return Err(format!("{kit} proof artifact is empty"));
    }
    let recomputed = blake3_512_of(&bytes);
    if recomputed != bundle_cid {
        return Err(format!(
            "{kit} proof artifact CID drift:\n  got:  {recomputed}\n  want: {bundle_cid}"
        ));
    }

    let pool = provekit_verifier::load_all_proofs::run(artifact_dir);
    if !pool.load_errors.is_empty() {
        let details = pool
            .load_errors
            .iter()
            .map(|e| format!("{}: {}", e.proof_path, e.reason))
            .collect::<Vec<_>>()
            .join("\n  ");
        return Err(format!(
            "{kit} proof artifact has load errors:\n  {details}"
        ));
    }

    let members = pool
        .bundle_members
        .get(bundle_cid)
        .ok_or_else(|| format!("{kit} proof artifact did not load bundle {bundle_cid}"))?;
    if members.is_empty() {
        return Err(format!("{kit} proof artifact loaded with zero members"));
    }

    let contract_cids = contract_content_cids_from_pool(&pool)?;
    if contract_cids.is_empty() {
        return Err(format!("{kit} proof artifact has no contract mementos"));
    }
    let derived_contract_set_cid = compute_contract_set_cid(contract_cids.clone());

    Ok(LiveProofArtifactValidation {
        proof_bytes_len: bytes.len(),
        proof_member_count: members.len(),
        proof_contract_count: contract_cids.len(),
        derived_contract_set_cid,
    })
}

fn bootstrap_self_contract_toolchains(profile: Profile) -> Result<()> {
    for target in self_contract_bootstrap_targets(profile) {
        println!("  bootstrap: make {target}");
        let cmd = vec!["make".to_string(), target.to_string()];
        let proc = run_cmd(&cmd, &repo_root(), Duration::from_secs(900));
        if proc.code != 0 {
            return Err(command_error(&cmd, &proc));
        }
    }
    Ok(())
}

fn mint_live_self_contract_artifact(
    producer: SelfContractProducer,
) -> Result<LiveSelfContractArtifact> {
    let tmp = TempDir::new(&format!(
        "pk_{}_self_contract_bootstrap",
        producer.attestation_lang
    ))?;
    let cmd = vec![
        provekit_bin().display().to_string(),
        "mint".to_string(),
        "--kit".to_string(),
        producer.mint_kit_alias.to_string(),
        "--quiet".to_string(),
        "--no-attest".to_string(),
        "--out".to_string(),
        tmp.path().display().to_string(),
    ];
    let proc = run_cmd(&cmd, &repo_root(), Duration::from_secs(600));
    if proc.code != 0 {
        return Err(command_error(&cmd, &proc));
    }

    let parsed = parse_mint_stdout(&proc.stdout)?;
    if parsed.contract_set_cid == EMPTY_CONTRACT_SET_CID {
        return Err("live mint produced the empty self-contract set".to_string());
    }

    let proof = validate_live_proof_artifact(producer.kit, tmp.path(), &parsed.bundle_cid)?;
    if proof.derived_contract_set_cid != parsed.contract_set_cid {
        return Err(format!(
            "{} proof artifact contractSetCid drift:\n  got:  {}\n  want: {}",
            producer.kit, proof.derived_contract_set_cid, parsed.contract_set_cid
        ));
    }

    Ok(LiveSelfContractArtifact {
        kit: producer.kit,
        attestation_lang: producer.attestation_lang,
        bundle_cid: parsed.bundle_cid,
        contract_set_cid: parsed.contract_set_cid,
        proof_bytes_len: proof.proof_bytes_len,
        proof_member_count: proof.proof_member_count,
        proof_contract_count: proof.proof_contract_count,
    })
}

fn verify_self_contract_attestation(
    attestation: &KitSelfContractAttestation,
    observed_contract_set_cid: &str,
) -> Result<()> {
    let verifier = repo_root().join("tools/foundation-keygen/target/release/verify-self-contracts");
    let cmd = vec![
        verifier.display().to_string(),
        attestation.path.display().to_string(),
        observed_contract_set_cid.to_string(),
    ];
    let proc = run_cmd(&cmd, &repo_root(), Duration::from_secs(30));
    if proc.code != 0 {
        return Err(command_error(&cmd, &proc));
    }
    Ok(())
}

fn sign_self_contract_attestation(artifact: &LiveSelfContractArtifact) -> Result<()> {
    let signer = repo_root().join("tools/foundation-keygen/target/release/sign-self-contracts");
    let cmd = vec![
        signer.display().to_string(),
        artifact.attestation_lang.to_string(),
        artifact.bundle_cid.clone(),
        artifact.contract_set_cid.clone(),
    ];
    let proc = run_cmd(&cmd, &repo_root(), Duration::from_secs(30));
    if proc.code != 0 {
        return Err(command_error(&cmd, &proc));
    }
    Ok(())
}

fn assert_live_artifact_matches_attestation(
    artifact: &LiveSelfContractArtifact,
    attestation: &KitSelfContractAttestation,
) -> Result<()> {
    if artifact.attestation_lang != attestation.attestation_lang {
        return Err(format!(
            "{} live artifact lang={} but attestation lang={}",
            artifact.kit, artifact.attestation_lang, attestation.attestation_lang
        ));
    }
    if artifact.contract_set_cid != attestation.contract_set_cid {
        return Err(format!(
            "{} live contractSetCid does not match pinned attestation:\n  got:  {}\n  want: {}",
            artifact.kit, artifact.contract_set_cid, attestation.contract_set_cid
        ));
    }
    // Bundle CIDs are representation CIDs and are signed for provenance, but
    // spec #94 makes contractSetCid the trust comparison. The verifier below
    // validates the pinned attestation signature and compares that signed
    // contractSetCid with the freshly-minted artifact.
    verify_self_contract_attestation(attestation, &artifact.contract_set_cid)?;
    Ok(())
}

fn load_fixtures() -> Result<FixtureToml> {
    let raw = fs::read_to_string(fixtures_toml())
        .map_err(|e| format!("read {}: {e}", fixtures_toml().display()))?;
    toml::from_str(&raw).map_err(|e| format!("parse {}: {e}", fixtures_toml().display()))
}

fn assert_catalog_pin(f: &FixtureToml) -> Result<()> {
    if f.catalog_version != EXPECTED_CATALOG_VERSION {
        return Err(format!(
            "fixtures.toml catalog_version={:?}; expected {:?}",
            f.catalog_version, EXPECTED_CATALOG_VERSION
        ));
    }
    if f.catalog_cid != EXPECTED_CATALOG_CID {
        return Err("fixtures.toml catalog_cid does not match v1.6.2".to_string());
    }

    let catalog_text = fs::read_to_string(catalog_json())
        .map_err(|e| format!("read {}: {e}", catalog_json().display()))?;
    let catalog: serde_json::Value =
        serde_json::from_str(&catalog_text).map_err(|e| format!("parse catalog JSON: {e}"))?;
    let version = catalog
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if version != EXPECTED_CATALOG_VERSION {
        return Err(format!(
            "protocol catalog version={version:?}; expected {EXPECTED_CATALOG_VERSION:?}"
        ));
    }
    Ok(())
}

fn assert_fixture_hash_pins(f: &FixtureToml) -> Result<()> {
    for fixture in &f.fixture {
        let got = blake3_512_of(fixture.jcs.as_bytes());
        if got != fixture.hash {
            return Err(format!(
                "fixture `{}` hash pin drift:\n  got:  {}\n  want: {}",
                fixture.name, got, fixture.hash
            ));
        }
    }
    Ok(())
}

fn make_fixture_map(f: FixtureToml) -> std::collections::BTreeMap<String, Fixture> {
    f.fixture
        .into_iter()
        .map(|fixture| (fixture.name.clone(), fixture))
        .collect()
}

fn rust_emit_cid(name: &str) -> Result<String> {
    let root = repo_root();
    let tmp = TempDir::new("pk_rust_conformance")?;
    write_file(
        &tmp.path().join("Cargo.toml"),
        &format!(
            r#"[package]
name = "pk-rust-conformance"
version = "0.0.0"
edition = "2021"

[dependencies]
provekit-canonicalizer = {{ path = "{}" }}
provekit-ir-symbolic = {{ path = "{}" }}
"#,
            root.join("implementations/rust/provekit-canonicalizer")
                .display(),
            root.join("implementations/rust/provekit-ir-symbolic")
                .display()
        ),
    )?;
    let code = format!(
        r#"
use provekit_canonicalizer::{{blake3_512_of, encode_jcs, Value}};
use provekit_ir_symbolic::serialize::formula_to_value;
use provekit_ir_symbolic::{{
    and_, eq, gte, implies, lt, make_var, num, str_const, Int, Term,
}};
use std::rc::Rc;

fn parse_int_arg(arg: Rc<Term>) -> Rc<Term> {{
    Rc::new(Term::Ctor {{
        name: "parse_int".into(),
        args: vec![arg],
    }})
}}

fn main() {{
    let jcs = match {} {{
        "eq_atomic" => {{
            let f = eq(parse_int_arg(str_const("42")), num(42));
            encode_jcs(&formula_to_value(&f))
        }}
        "pattern1_bounded_loop" => {{
            let x = make_var("x");
            let body = implies(
                and_(vec![gte(x.clone(), num(0)), lt(x.clone(), num(100))]),
                gte(x, num(0)),
            );
            let q = provekit_ir_symbolic::Formula::Quantifier {{
                kind: "forall".into(),
                name: "x".into(),
                sort: Int(),
                body,
            }};
            encode_jcs(&formula_to_value(&q))
        }}
        "contract_decl" => {{
            let pre = gte(make_var("x"), num(0));
            let value = Value::array(vec![Value::object([
                ("kind", Value::string("contract")),
                ("name", Value::string("parseInt")),
                ("outBinding", Value::string("out")),
                ("pre", formula_to_value(&pre)),
            ])]);
            encode_jcs(&value)
        }}
        _ => panic!("unknown fixture"),
    }};
    print!("{{}}", blake3_512_of(jcs.as_bytes()));
}}
"#,
        json_lit(name)
    );
    write_file(&tmp.path().join("src/main.rs"), &code)?;
    command_stdout(
        &["cargo".into(), "run".into(), "--quiet".into()],
        tmp.path(),
        Duration::from_secs(180),
    )
}

fn python_emit_cid(name: &str) -> Result<String> {
    let root = repo_root();
    let py_src = root.join("implementations/python/provekit-lift-py-tests/src");
    let code = r#"
import sys
from provekit_lift_py_tests.canonicalizer import encode_jcs, blake3_512_of
from provekit_lift_py_tests.ir import (
    BridgeDecl, ContractDecl, Int, _Quantifier, and_, bridge_decl_to_value,
    contract_decl_to_value, ctor, declarations_to_value, eq, formula_to_value,
    gte, implies, lt, make_var, num, str_const,
)

name = sys.argv[1]
if name == "eq_atomic":
    jcs = encode_jcs(formula_to_value(eq(ctor("parse_int", [str_const("42")]), num(42))))
elif name == "pattern1_bounded_loop":
    x = make_var("x")
    body = implies(and_([gte(x, num(0)), lt(x, num(100))]), gte(x, num(0)))
    jcs = encode_jcs(formula_to_value(_Quantifier("forall", "x", Int(), body)))
elif name == "contract_decl":
    pre = gte(make_var("x"), num(0))
    jcs = encode_jcs(declarations_to_value([ContractDecl(name="parseInt", pre=pre)]))
elif name == "bridge_decl_v1_1":
    bridge = BridgeDecl(
        name="myBridge",
        source_symbol="source",
        source_layer="c-kit",
        source_contract_cid="bafySource",
        target_contract_cid="bafyTarget",
        target_proof_cid="bafyProof",
        target_layer="coq",
        notes="some notes",
    )
    jcs = encode_jcs(bridge_decl_to_value(bridge))
else:
    raise SystemExit(f"unknown fixture: {name}")
print(blake3_512_of(jcs.encode("utf-8")), end="")
"#;
    let p = run_cmd_env(
        &["python3".into(), "-c".into(), code.into(), name.into()],
        &root,
        Duration::from_secs(60),
        &[("PYTHONPATH", OsString::from(py_src))],
    );
    if p.code != 0 {
        Err(command_error(
            &["python3".into(), "-c".into(), "<python-kit-adapter>".into()],
            &p,
        ))
    } else {
        Ok(p.stdout.trim().to_string())
    }
}

fn go_emit_cid(name: &str) -> Result<String> {
    let root = repo_root();
    let module = root.join("implementations/go/provekit-ir-symbolic");
    let unique = format!(
        "pk_conformance_{}_{}.go",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    );
    let src = module.join(unique);
    let code = format!(
        r#"
package main

import (
    "encoding/json"
    "fmt"
    "strings"
    canon "github.com/tsavo/provekit/go/provekit-ir-symbolic/canonicalizer"
    ir "github.com/tsavo/provekit/go/provekit-ir-symbolic/ir"
)

func emit(raw []byte) {{
    var v any
    dec := json.NewDecoder(strings.NewReader(string(raw)))
    dec.UseNumber()
    if err := dec.Decode(&v); err != nil {{
        panic(err)
    }}
    out, err := canon.EncodeJCS(v)
    if err != nil {{
        panic(err)
    }}
    fmt.Print(canon.ComputeCID(out))
}}

func main() {{
    switch {} {{
    case "eq_atomic":
        f := ir.Eq(ir.MakeCtor("parse_int", []ir.IrTerm{{ir.StrConst("42")}}, ir.Int), ir.Num(42))
        out, _ := json.Marshal(f)
        emit(out)
    case "pattern1_bounded_loop":
        f := ir.ForAllNamed("x", ir.Int, func(x ir.IrTerm) ir.IrFormula {{
            return ir.Implies(ir.And(ir.Gte(x, ir.Num(0)), ir.Lt(x, ir.Num(100))), ir.Gte(x, ir.Num(0)))
        }})
        out, _ := json.Marshal(f)
        emit(out)
    case "contract_decl":
        ir.ResetCollector()
        finish := ir.BeginCollecting()
        ir.Contract("parseInt", ir.ContractArgs{{Pre: ir.Gte(ir.MakeVar("x", ir.Int), ir.Num(0))}})
        decls := finish()
        out, _ := ir.MarshalDeclarations(decls)
        emit(out)
    case "bridge_decl_v1_1":
        b := ir.BridgeDeclaration{{
            Name: "myBridge",
            SourceSymbol: "source",
            SourceLayer: "c-kit",
            SourceContractCid: "bafySource",
            TargetContractCid: "bafyTarget",
            TargetProofCid: "bafyProof",
            TargetLayer: "coq",
            Notes: "some notes",
        }}
        out, _ := json.Marshal(b)
        emit(out)
    default:
        panic("unknown fixture")
    }}
}}
"#,
        json_lit(name)
    );
    write_file(&src, &code)?;
    let result = command_stdout(
        &[
            "go".into(),
            "run".into(),
            src.file_name().unwrap().to_string_lossy().into_owned(),
        ],
        &module,
        Duration::from_secs(60),
    );
    let _ = fs::remove_file(&src);
    result
}

fn c_emit_cid(name: &str) -> Result<String> {
    let root = repo_root();
    let c_dir = root.join("implementations/c/provekit-ir");
    let b3 = root.join("tools/blake3-vendored");
    let body = match name {
        "eq_atomic" => {
            r#"
            pk_term *arg = pk_term_const_str("42", pk_sort_primitive("String"));
            pk_term *args1[] = { arg };
            pk_term *lhs = pk_term_ctor_new("parse_int", args1, 1);
            pk_term *rhs = pk_term_const_int(42, pk_sort_primitive("Int"));
            pk_term *args2[] = { lhs, rhs };
            pk_formula *f = pk_formula_atomic_new("=", args2, 2);
            pk_emit_formula(buf, f);
            pk_formula_free(f);
"#
        }
        "pattern1_bounded_loop" => {
            r#"
            pk_term *x1 = pk_term_var_new("x");
            pk_term *zero1 = pk_term_const_int(0, pk_sort_primitive("Int"));
            pk_term *lower_args[] = { x1, zero1 };
            pk_formula *lower = pk_formula_atomic_new("≥", lower_args, 2);
            pk_term *x2 = pk_term_var_new("x");
            pk_term *hundred = pk_term_const_int(100, pk_sort_primitive("Int"));
            pk_term *upper_args[] = { x2, hundred };
            pk_formula *upper = pk_formula_atomic_new("<", upper_args, 2);
            pk_formula *conj[] = { lower, upper };
            pk_formula *ant = pk_formula_connective_new("and", conj, 2);
            pk_term *x3 = pk_term_var_new("x");
            pk_term *zero2 = pk_term_const_int(0, pk_sort_primitive("Int"));
            pk_term *inner_args[] = { x3, zero2 };
            pk_formula *inner = pk_formula_atomic_new("≥", inner_args, 2);
            pk_formula *impl[] = { ant, inner };
            pk_formula *body = pk_formula_connective_new("implies", impl, 2);
            pk_formula *q = pk_formula_quantifier_new("forall", "x", pk_sort_primitive("Int"), body);
            pk_emit_formula(buf, q);
            pk_formula_free(q);
"#
        }
        "contract_decl" => {
            r#"
            pk_term *x = pk_term_var_new("x");
            pk_term *zero = pk_term_const_int(0, pk_sort_primitive("Int"));
            pk_term *args[] = { x, zero };
            pk_formula *pre = pk_formula_atomic_new("≥", args, 2);
            pk_decl *d = pk_decl_contract_new("parseInt", "out", pre, NULL, NULL);
            pk_decl *decls[] = { d };
            pk_emit_decls(buf, decls, 1);
            pk_decl_free(d);
"#
        }
        "bridge_decl_v1_1" => {
            r#"
            pk_decl *d = pk_decl_bridge_new("myBridge", "source", "c-kit", "bafySource", "bafyTarget", "bafyProof", "coq", "some notes");
            pk_emit_decl(buf, d);
            pk_decl_free(d);
"#
        }
        _ => return Err(format!("unknown C fixture `{name}`")),
    };

    let tmp = TempDir::new("pk_c_conformance")?;
    let src = tmp.path().join("main.c");
    let out = tmp.path().join("main");
    write_file(
        &src,
        &format!(
            r#"
#include "provekit/ir.h"
#include <stdio.h>
#include <stdlib.h>
int main(void) {{
    pk_buffer *buf = pk_buffer_new();
    {body}
    char *cid = pk_hash_jcs(buf->data);
    printf("%s", cid);
    free(cid);
    pk_buffer_free(buf);
    return 0;
}}
"#
        ),
    )?;
    let mut cmd = vec![
        "cc".to_string(),
        "-std=c11".to_string(),
        "-DBLAKE3_NO_AVX2".to_string(),
        "-DBLAKE3_NO_AVX512".to_string(),
        "-DBLAKE3_NO_SSE2".to_string(),
        "-DBLAKE3_NO_SSE41".to_string(),
        "-DBLAKE3_USE_NEON=0".to_string(),
        "-I".to_string(),
        c_dir.join("include").display().to_string(),
        "-I".to_string(),
        b3.display().to_string(),
        src.display().to_string(),
    ];
    for rel in ["src/ir.c", "src/jcs.c", "src/hash.c"] {
        cmd.push(c_dir.join(rel).display().to_string());
    }
    for rel in ["blake3.c", "blake3_dispatch.c", "blake3_portable.c"] {
        cmd.push(b3.join(rel).display().to_string());
    }
    cmd.extend(["-o".to_string(), out.display().to_string()]);
    let p = run_cmd(&cmd, &root, Duration::from_secs(60));
    if p.code != 0 {
        return Err(command_error(&cmd, &p));
    }
    command_stdout(&[out.display().to_string()], &root, Duration::from_secs(20))
}

fn cpp_emit_cid(name: &str) -> Result<String> {
    let root = repo_root();
    let ir_include = root.join("implementations/cpp/provekit-ir-symbolic/include");
    let canon = root.join("implementations/cpp/provekit/canonicalizer");
    let b3 = root.join("tools/blake3-vendored");
    let body = match name {
        "eq_atomic" => {
            r#"
            auto lhs = std::make_shared<Term>(Term{CtorTerm{"parse_int", {str_const("42")}}});
            auto rhs = num(42);
            auto f = std::make_shared<Formula>(Formula{AtomicFormula{"=", {lhs, rhs}}});
            write_formula(out, *f);
"#
        }
        "pattern1_bounded_loop" => {
            r#"
            auto x1 = make_var("x");
            auto x2 = make_var("x");
            auto x3 = make_var("x");
            auto lower = std::make_shared<Formula>(Formula{AtomicFormula{"≥", {x1, num(0)}}});
            auto upper = std::make_shared<Formula>(Formula{AtomicFormula{"<", {x2, num(100)}}});
            auto ant = std::make_shared<Formula>(Formula{ConnectiveFormula{"and", {lower, upper}}});
            auto inner = std::make_shared<Formula>(Formula{AtomicFormula{"≥", {x3, num(0)}}});
            auto body = std::make_shared<Formula>(Formula{ConnectiveFormula{"implies", {ant, inner}}});
            auto q = std::make_shared<Formula>(Formula{QuantifierFormula{"forall", "x", Int(), body}});
            write_formula(out, *q);
"#
        }
        "contract_decl" => {
            r#"
            auto pre = std::make_shared<Formula>(Formula{AtomicFormula{"≥", {make_var("x"), num(0)}}});
            std::vector<ContractDecl> decls{ContractDecl{"parseInt", pre, nullptr, nullptr, "out", nullptr}};
            out << marshal_declarations(decls);
"#
        }
        "bridge_decl_v1_1" => {
            r#"
            BridgeDecl b;
            b.name = "myBridge";
            b.source_symbol = "source";
            b.source_layer = "c-kit";
            b.source_contract_cid = "bafySource";
            b.target_contract_cid = "bafyTarget";
            b.target_proof_cid = "bafyProof";
            b.target_layer = "coq";
            b.notes = "some notes";
            write_bridge_decl(out, b);
"#
        }
        _ => return Err(format!("unknown C++ fixture `{name}`")),
    };
    let tmp = TempDir::new("pk_cpp_conformance")?;
    let src = tmp.path().join("main.cpp");
    let out_bin = tmp.path().join("main");
    write_file(
        &src,
        &format!(
            r#"
#include "provekit/ir.hpp"
#include "hash.hpp"
#include <iostream>
#include <sstream>
using namespace provekit::ir;
int main() {{
    std::ostringstream out;
    {body}
    std::cout << provekit::canonicalizer::compute_cid(out.str());
    return 0;
}}
"#
        ),
    )?;

    let b3_flags = [
        "-DBLAKE3_NO_AVX2",
        "-DBLAKE3_NO_AVX512",
        "-DBLAKE3_NO_SSE2",
        "-DBLAKE3_NO_SSE41",
        "-DBLAKE3_USE_NEON=0",
    ];
    let mut objects = Vec::new();
    for rel in ["blake3.c", "blake3_dispatch.c", "blake3_portable.c"] {
        let obj = tmp.path().join(format!("{rel}.o").replace('/', "_"));
        let mut cmd = vec!["cc".to_string(), "-std=c11".to_string()];
        cmd.extend(b3_flags.iter().map(|s| s.to_string()));
        cmd.extend([
            "-I".to_string(),
            b3.display().to_string(),
            "-c".to_string(),
            b3.join(rel).display().to_string(),
            "-o".to_string(),
            obj.display().to_string(),
        ]);
        let p = run_cmd(&cmd, &root, Duration::from_secs(30));
        if p.code != 0 {
            return Err(command_error(&cmd, &p));
        }
        objects.push(obj);
    }
    let hash_obj = tmp.path().join("hash.o");
    let hash_cmd = vec![
        "c++".to_string(),
        "-std=c++17".to_string(),
        "-I".to_string(),
        canon.display().to_string(),
        "-I".to_string(),
        b3.display().to_string(),
        "-c".to_string(),
        canon.join("hash.cpp").display().to_string(),
        "-o".to_string(),
        hash_obj.display().to_string(),
    ];
    let p = run_cmd(&hash_cmd, &root, Duration::from_secs(30));
    if p.code != 0 {
        return Err(command_error(&hash_cmd, &p));
    }
    let mut link = vec![
        "c++".to_string(),
        "-std=c++17".to_string(),
        "-I".to_string(),
        ir_include.display().to_string(),
        "-I".to_string(),
        canon.display().to_string(),
        "-I".to_string(),
        b3.display().to_string(),
        src.display().to_string(),
        hash_obj.display().to_string(),
    ];
    link.extend(objects.iter().map(|p| p.display().to_string()));
    link.extend(["-o".to_string(), out_bin.display().to_string()]);
    let p = run_cmd(&link, &root, Duration::from_secs(60));
    if p.code != 0 {
        return Err(command_error(&link, &p));
    }
    command_stdout(
        &[out_bin.display().to_string()],
        &root,
        Duration::from_secs(20),
    )
}

fn zig_tool() -> Result<PathBuf> {
    let bundled = repo_root().join("zig-toolchain/zig");
    if bundled.exists() {
        return Ok(bundled);
    }
    let p = run_cmd(
        &["sh".into(), "-c".into(), "command -v zig".into()],
        &repo_root(),
        Duration::from_secs(5),
    );
    if p.code == 0 && !p.stdout.trim().is_empty() {
        Ok(PathBuf::from(p.stdout.trim()))
    } else {
        Err("required tool not found: zig".to_string())
    }
}

fn zig_emit_cid(name: &str) -> Result<String> {
    let root = repo_root();
    let zig = zig_tool()?;
    let src_dir = root.join("implementations/zig/provekit-ir/src");
    let body = match name {
        "eq_atomic" => {
            r#"
            const ctor_args = [_]provekit.Term{provekit.Str("42")};
            const lhs = provekit.Ctor("parse_int", &ctor_args);
            const rhs = provekit.Num(42);
            const atomic_args = [_]provekit.Term{ lhs, rhs };
            const value = provekit.Atomic("=", &atomic_args);
            const jcs = try provekit.jcsStringify(std.heap.page_allocator, value);
"#
        }
        "pattern1_bounded_loop" => {
            r#"
            const lower_args = [_]provekit.Term{ provekit.Var("x"), provekit.Num(0) };
            const lower = provekit.Atomic("≥", &lower_args);
            const upper_args = [_]provekit.Term{ provekit.Var("x"), provekit.Num(100) };
            const upper = provekit.Atomic("<", &upper_args);
            const conj_args = [_]provekit.Formula{ lower, upper };
            const ant = provekit.And(&conj_args);
            const inner_args = [_]provekit.Term{ provekit.Var("x"), provekit.Num(0) };
            const inner = provekit.Atomic("≥", &inner_args);
            const impl_args = [_]provekit.Formula{ ant, inner };
            const body = provekit.Implies(&impl_args);
            const value = provekit.Forall("x", provekit.Sort.Int, &body);
            const jcs = try provekit.jcsStringify(std.heap.page_allocator, value);
"#
        }
        "contract_decl" => {
            r#"
            const pre_args = [_]provekit.Term{ provekit.Var("x"), provekit.Num(0) };
            const pre = provekit.Atomic("≥", &pre_args);
            const decl = provekit.Decl{ .contract = .{ .name = "parseInt", .out_binding = "out", .pre = pre } };
            const decls = [_]provekit.Decl{decl};
            const jcs = try provekit.jcsStringify(std.heap.page_allocator, &decls);
"#
        }
        "bridge_decl_v1_1" => {
            r#"
            const value = provekit.Decl{ .bridge = .{
                .name = "myBridge",
                .source_symbol = "source",
                .source_layer = "c-kit",
                .source_contract_cid = "bafySource",
                .target_contract_cid = "bafyTarget",
                .target_proof_cid = "bafyProof",
                .target_layer = "coq",
                .notes = "some notes",
            } };
            const jcs = try provekit.jcsStringify(std.heap.page_allocator, value);
"#
        }
        _ => return Err(format!("unknown Zig fixture `{name}`")),
    };
    let tmp = TempDir::new("pk_zig_conformance")?;
    fs::copy(src_dir.join("root.zig"), tmp.path().join("root.zig"))
        .map_err(|e| format!("copy root.zig: {e}"))?;
    fs::copy(
        src_dir.join("cross_kit_bridges.zig"),
        tmp.path().join("cross_kit_bridges.zig"),
    )
    .map_err(|e| format!("copy cross_kit_bridges.zig: {e}"))?;
    write_file(
        &tmp.path().join("main.zig"),
        &format!(
            r#"
const std = @import("std");
const provekit = @import("provekit-ir");
pub fn main(init: std.process.Init) !void {{
    {body}
    defer std.heap.page_allocator.free(jcs);
    const cid = try provekit.jcsHash(std.heap.page_allocator, jcs);
    defer std.heap.page_allocator.free(cid);
    var write_buf: [4096]u8 = undefined;
    var stdout_file = std.Io.File.stdout().writerStreaming(init.io, &write_buf);
    var stdout_writer = &stdout_file.interface;
    try stdout_writer.print("{{s}}", .{{cid}});
    try stdout_writer.flush();
}}
"#
        ),
    )?;
    write_file(
        &tmp.path().join("build.zig"),
        r#"
const std = @import("std");
pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});
    const provekit_ir = b.createModule(.{
        .root_source_file = b.path("root.zig"),
        .target = target,
        .optimize = optimize,
    });
    const exe_mod = b.createModule(.{
        .root_source_file = b.path("main.zig"),
        .target = target,
        .optimize = optimize,
        .imports = &.{
            .{ .name = "provekit-ir", .module = provekit_ir },
        },
    });
    const exe = b.addExecutable(.{
        .name = "main",
        .root_module = exe_mod,
    });
    b.installArtifact(exe);
}
"#,
    )?;
    let p = run_cmd(
        &[
            zig.display().to_string(),
            "build".into(),
            "--prefix".into(),
            ".".into(),
        ],
        tmp.path(),
        Duration::from_secs(120),
    );
    if p.code != 0 {
        return Err(command_error(
            &[
                zig.display().to_string(),
                "build".into(),
                "--prefix".into(),
                ".".into(),
            ],
            &p,
        ));
    }
    command_stdout(
        &[tmp.path().join("bin/main").display().to_string()],
        tmp.path(),
        Duration::from_secs(20),
    )
}

fn csharp_emit_cid(name: &str) -> Result<String> {
    let root = repo_root();
    let csharp = root.join("implementations/csharp");
    let body = match name {
        "eq_atomic" => {
            r#"
            var lhs = Terms.Ctor("parse_int", [Terms.StrConst("42")]);
            var rhs = Terms.Num(42);
            var jcs = Jcs.Encode(Serialize.FormulaToValue(Predicates.Eq(lhs, rhs)));
            Console.Write(Provekit.Canonicalizer.Hash.Blake3_512Utf8(jcs));
"#
        }
        "pattern1_bounded_loop" => {
            r#"
            var x = Terms.Var("x");
            var lower = Predicates.Gte(x, Terms.Num(0));
            var upper = Predicates.Lt(x, Terms.Num(100));
            var ant = Predicates.And(lower, upper);
            var inner = Predicates.Gte(x, Terms.Num(0));
            var q = new QuantifierFormula("forall", "x", Sort.Int, Predicates.Implies(ant, inner));
            var jcs = Jcs.Encode(Serialize.FormulaToValue(q));
            Console.Write(Provekit.Canonicalizer.Hash.Blake3_512Utf8(jcs));
"#
        }
        "contract_decl" => {
            r#"
            var pre = Predicates.Gte(Terms.Var("x"), Terms.Num(0));
            var value = Value.Array(Value.Object(
                ("kind", Value.String("contract")),
                ("name", Value.String("parseInt")),
                ("outBinding", Value.String("out")),
                ("pre", Serialize.FormulaToValue(pre))
            ));
            var jcs = Jcs.Encode(value);
            Console.Write(Provekit.Canonicalizer.Hash.Blake3_512Utf8(jcs));
"#
        }
        "bridge_decl_v1_1" => {
            r#"
            var bridge = new BridgeDeclaration("myBridge", "source", "c-kit", "bafySource", "bafyTarget", "bafyProof", "coq", "some notes");
            var jcs = Jcs.Encode(Serialize.BridgeDeclarationToValue(bridge));
            Console.Write(Provekit.Canonicalizer.Hash.Blake3_512Utf8(jcs));
"#
        }
        _ => return Err(format!("unknown C# fixture `{name}`")),
    };
    let tmp = TempDir::new("pk_cs_conformance")?;
    write_file(
        &tmp.path().join("pk_cs_conformance.csproj"),
        &format!(
            r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net10.0</TargetFramework>
    <ImplicitUsings>enable</ImplicitUsings>
    <Nullable>enable</Nullable>
  </PropertyGroup>
  <ItemGroup>
    <ProjectReference Include="{}" />
    <ProjectReference Include="{}" />
  </ItemGroup>
</Project>
"#,
            csharp.join("Provekit.IR/Provekit.IR.csproj").display(),
            csharp
                .join("Provekit.Canonicalizer/Provekit.Canonicalizer.csproj")
                .display()
        ),
    )?;
    write_file(
        &tmp.path().join("Program.cs"),
        &format!(
            r#"
using Provekit.Canonicalizer;
using Provekit.IR;

{body}
"#
        ),
    )?;
    command_stdout(
        &[
            "dotnet".into(),
            "run".into(),
            "--project".into(),
            "pk_cs_conformance.csproj".into(),
        ],
        tmp.path(),
        Duration::from_secs(120),
    )
}

fn ruby_emit_cid(name: &str) -> Result<String> {
    let root = repo_root();
    let code = r#"
require "provekit"

def emit(jcs)
  print Provekit::Blake3.hex(jcs)
end

case ARGV.fetch(0)
when "eq_atomic"
  lhs = Provekit::IR.ctor("parse_int", Provekit::IR.str("42"))
  rhs = Provekit::IR.num(42)
  emit Provekit::IR::Jcs.encode(Provekit::IR.eq(lhs, rhs))
when "pattern1_bounded_loop"
  x = Provekit::IR.var(name: "x")
  body = Provekit::IR.implies(
    Provekit::IR.and(
      Provekit::IR.gte(x, Provekit::IR.num(0)),
      Provekit::IR.lt(x, Provekit::IR.num(100)),
    ),
    Provekit::IR.gte(x, Provekit::IR.num(0)),
  )
  q = Provekit::IR.forall(name: "x", sort: Provekit::IR::PrimitiveSort.Int, body: body)
  emit Provekit::IR::Jcs.encode(q)
when "contract_decl"
  pre = Provekit::IR.gte(Provekit::IR.var(name: "x"), Provekit::IR.num(0))
  d = Provekit::IR::ContractDecl.new(name: "parseInt", pre: pre)
  emit Provekit::IR.marshal_declarations([d])
when "bridge_decl_v1_1"
  d = Provekit::IR::Bridge.new(
    name: "myBridge",
    source_symbol: "source",
    source_layer: "c-kit",
    source_contract_cid: "bafySource",
    target_contract_cid: "bafyTarget",
    target_proof_cid: "bafyProof",
    target_layer: "coq",
    notes: "some notes",
  )
  emit Provekit::IR.marshal_declarations([d])[1...-1]
else
  abort "unknown fixture"
end
"#;
    command_stdout(
        &ruby_bundle_exec_cmd(&["-Ilib", "-e", code, name]),
        &root.join("implementations/ruby"),
        Duration::from_secs(60),
    )
}

fn php_emit_cid(name: &str) -> Result<String> {
    let root = repo_root();
    let code = r#"
require "provekit-ir-symbolic/src/Canonicalizer/Jcs.php";
require "provekit-ir-symbolic/src/Canonicalizer/Blake3.php";
require "provekit-ir-symbolic/src/Ir/Term.php";
require "provekit-ir-symbolic/src/Ir/Formula.php";
require "provekit-ir-symbolic/src/Ir/Declaration.php";

function emit($value) {
    echo \ProvekIt\Canonicalizer\Blake3::cid(\ProvekIt\Canonicalizer\Jcs::encode($value));
}

$name = $argv[1] ?? "";
switch ($name) {
case "eq_atomic":
    emit(\ProvekIt\Ir\Eq(
        \ProvekIt\Ir\Ctor("parse_int", \ProvekIt\Ir\Str("42")),
        \ProvekIt\Ir\Num(42)
    ));
    break;
case "pattern1_bounded_loop":
    $x = \ProvekIt\Ir\V("x");
    $body = \ProvekIt\Ir\Implies(
        \ProvekIt\Ir\And_(
            \ProvekIt\Ir\Gte($x, \ProvekIt\Ir\Num(0)),
            \ProvekIt\Ir\Lt($x, \ProvekIt\Ir\Num(100))
        ),
        \ProvekIt\Ir\Gte($x, \ProvekIt\Ir\Num(0))
    );
    emit(\ProvekIt\Ir\ForAll("x", \ProvekIt\Ir\Sort::Int(), $body));
    break;
case "contract_decl":
    $pre = \ProvekIt\Ir\Gte(\ProvekIt\Ir\V("x"), \ProvekIt\Ir\Num(0));
    emit([new \ProvekIt\Ir\ContractDecl("parseInt", "out", $pre)]);
    break;
case "bridge_decl_v1_1":
    emit(new \ProvekIt\Ir\BridgeDecl(
        "myBridge",
        "source",
        "c-kit",
        "bafySource",
        "bafyTarget",
        "bafyProof",
        "coq",
        "some notes"
    ));
    break;
default:
    fwrite(STDERR, "unknown fixture\n");
    exit(1);
}
"#;
    command_stdout(
        &["php".into(), "-r".into(), code.into(), name.into()],
        &root.join("implementations/php"),
        Duration::from_secs(60),
    )
}

fn java_classpath() -> Result<String> {
    let root = repo_root();
    let java_root = root.join("implementations/java");
    let package_cmd = vec![
        "mvn".to_string(),
        "-q".to_string(),
        "-f".to_string(),
        "implementations/java/pom.xml".to_string(),
        "-pl".to_string(),
        "provekit-ir,provekit-claim-envelope".to_string(),
        "-am".to_string(),
        "package".to_string(),
        "-DskipTests".to_string(),
    ];
    let p = run_cmd(&package_cmd, &root, Duration::from_secs(180));
    if p.code != 0 {
        return Err(command_error(&package_cmd, &p));
    }
    let cp_file = java_root.join("provekit-claim-envelope/target/classpath.txt");
    let dep_cmd = vec![
        "mvn".to_string(),
        "-q".to_string(),
        "-f".to_string(),
        "implementations/java/provekit-claim-envelope/pom.xml".to_string(),
        "dependency:build-classpath".to_string(),
        format!("-Dmdep.outputFile={}", cp_file.display()),
    ];
    let p = run_cmd(&dep_cmd, &root, Duration::from_secs(120));
    if p.code != 0 {
        return Err(command_error(&dep_cmd, &p));
    }
    let mut parts = vec![
        java_root
            .join("provekit-ir/target/classes")
            .display()
            .to_string(),
        java_root
            .join("provekit-claim-envelope/target/classes")
            .display()
            .to_string(),
    ];
    if let Ok(extra) = fs::read_to_string(cp_file) {
        let trimmed = extra.trim();
        if !trimmed.is_empty() {
            parts.push(trimmed.to_string());
        }
    }
    Ok(parts.join(if cfg!(windows) { ";" } else { ":" }))
}

fn java_emit_cid(name: &str) -> Result<String> {
    let cp = java_classpath()?;
    let tmp = TempDir::new("pk_java_conformance")?;
    let code = format!(
        r#"
import com.provekit.ir.*;
import com.provekit.claimenvelope.Blake3;
import java.nio.charset.StandardCharsets;

public class PkJavaConformance {{
  private static void emit(String jcs) {{
    System.out.print(Blake3.blake3_512(jcs.getBytes(StandardCharsets.UTF_8)));
  }}

  public static void main(String[] args) {{
    switch ({}) {{
      case "eq_atomic" -> {{
        Term lhs = Term.ctor("parse_int", new Term[]{{ Term.const_("42", Sort.String) }}, Sort.Int);
        Term rhs = Term.const_(42, Sort.Int);
        emit(Formula.atomic("=", lhs, rhs).toJson());
      }}
      case "pattern1_bounded_loop" -> {{
        Term x = Term.var_("x", Sort.Int);
        Formula lower = Formula.atomic("≥", x, Term.const_(0, Sort.Int));
        Formula upper = Formula.atomic("<", x, Term.const_(100, Sort.Int));
        Formula ant = Formula.and(lower, upper);
        Formula inner = Formula.atomic("≥", x, Term.const_(0, Sort.Int));
        emit(Formula.forall("x", Sort.Int, Formula.implies(ant, inner)).toJson());
      }}
      case "contract_decl" -> {{
        Term x = Term.var_("x", Sort.Int);
        Formula pre = Formula.atomic("≥", x, Term.const_(0, Sort.Int));
        Declaration.Contract d = new Declaration.Contract("parseInt", "out", pre, null, null, null);
        emit("[" + d.toJson() + "]");
      }}
      case "bridge_decl_v1_1" -> {{
        Declaration.Bridge b = new Declaration.Bridge(
          "myBridge", "source", "c-kit", "bafySource", "bafyTarget",
          "bafyProof", "coq", "some notes");
        emit(b.toJson());
      }}
      default -> throw new IllegalArgumentException("unknown fixture");
    }}
  }}
}}
"#,
        json_lit(name)
    );
    let src = tmp.path().join("PkJavaConformance.java");
    write_file(&src, &code)?;
    let p = run_cmd(
        &[
            "javac".into(),
            "-cp".into(),
            cp.clone(),
            src.display().to_string(),
        ],
        tmp.path(),
        Duration::from_secs(60),
    );
    if p.code != 0 {
        return Err(command_error(
            &[
                "javac".into(),
                "-cp".into(),
                cp.clone(),
                src.display().to_string(),
            ],
            &p,
        ));
    }
    command_stdout(
        &[
            "java".into(),
            "-cp".into(),
            format!(
                "{cp}{}{}",
                if cfg!(windows) { ";" } else { ":" },
                tmp.path().display()
            ),
            "PkJavaConformance".into(),
        ],
        tmp.path(),
        Duration::from_secs(60),
    )
}

fn swift_emit_cid(name: &str) -> Result<String> {
    let root = repo_root();
    command_stdout(
        &[
            "swift".into(),
            "run".into(),
            "conformance".into(),
            "--fixture".into(),
            name.into(),
        ],
        &root.join("implementations/swift"),
        Duration::from_secs(180),
    )
}

fn linux_direct_adapters() -> Vec<DirectAdapter> {
    vec![
        DirectAdapter {
            kit: "rust",
            emit: rust_emit_cid,
            fixtures: RUST_CORE_FIXTURES,
        },
        DirectAdapter {
            kit: "python",
            emit: python_emit_cid,
            fixtures: CORE_FIXTURES,
        },
        DirectAdapter {
            kit: "go",
            emit: go_emit_cid,
            fixtures: CORE_FIXTURES,
        },
        DirectAdapter {
            kit: "c",
            emit: c_emit_cid,
            fixtures: CORE_FIXTURES,
        },
        DirectAdapter {
            kit: "cpp",
            emit: cpp_emit_cid,
            fixtures: CORE_FIXTURES,
        },
        DirectAdapter {
            kit: "zig",
            emit: zig_emit_cid,
            fixtures: CORE_FIXTURES,
        },
        DirectAdapter {
            kit: "csharp",
            emit: csharp_emit_cid,
            fixtures: CORE_FIXTURES,
        },
        DirectAdapter {
            kit: "ruby",
            emit: ruby_emit_cid,
            fixtures: CORE_FIXTURES,
        },
        DirectAdapter {
            kit: "php",
            emit: php_emit_cid,
            fixtures: CORE_FIXTURES,
        },
        DirectAdapter {
            kit: "java",
            emit: java_emit_cid,
            fixtures: CORE_FIXTURES,
        },
    ]
}

fn swift_direct_adapters() -> Vec<DirectAdapter> {
    vec![DirectAdapter {
        kit: "swift",
        emit: swift_emit_cid,
        fixtures: CORE_FIXTURES,
    }]
}

fn linux_native_checks() -> Vec<NativeCheck> {
    let root = repo_root();
    vec![
        NativeCheck {
            kit: "rust",
            name: "rust bridge_v1_4 fixture CID",
            cmd: vec![
                "cargo".into(),
                "test".into(),
                "--release".into(),
                "--manifest-path".into(),
                "implementations/rust/Cargo.toml".into(),
                "-p".into(),
                "provekit-claim-envelope".into(),
                "--test".into(),
                "bridge_v14_roundtrip".into(),
            ],
            cwd: root.clone(),
            timeout: Duration::from_secs(300),
        },
        NativeCheck {
            kit: "typescript",
            name: "typescript fixture CIDs",
            cmd: vec![
                "pnpm".into(),
                "exec".into(),
                "vitest".into(),
                "run".into(),
                "implementations/typescript/src/canonicalizer/cross-impl-golden.test.ts".into(),
            ],
            cwd: root.clone(),
            timeout: Duration::from_secs(180),
        },
        NativeCheck {
            kit: "ruby",
            name: "ruby bridge_v1_4 fixture CID",
            cmd: ruby_bundle_exec_cmd(&["-Ilib", "-Itest", "test/test_bridge_v14.rb"]),
            cwd: root.join("implementations/ruby"),
            timeout: Duration::from_secs(120),
        },
        NativeCheck {
            kit: "java",
            name: "java bridge_v1_4 fixture CID",
            cmd: vec![
                "mvn".into(),
                "test".into(),
                "-q".into(),
                "-f".into(),
                "implementations/java/provekit-claim-envelope/pom.xml".into(),
                "-Dtest=BridgeV14RoundtripTest".into(),
            ],
            cwd: root.clone(),
            timeout: Duration::from_secs(180),
        },
        NativeCheck {
            kit: "csharp",
            name: "csharp bridge_v1_4 fixture CID",
            cmd: vec![
                "dotnet".into(),
                "test".into(),
                "implementations/csharp/Provekit.Tests/Provekit.Tests.csproj".into(),
                "--filter".into(),
                "BridgeV14".into(),
                "--nologo".into(),
                "--verbosity".into(),
                "quiet".into(),
            ],
            cwd: root,
            timeout: Duration::from_secs(180),
        },
    ]
}

fn swift_native_checks() -> Vec<NativeCheck> {
    let root = repo_root();
    vec![NativeCheck {
        kit: "swift",
        name: "swift conformance runner CID checks",
        cmd: vec!["swift".into(), "run".into(), "conformance".into()],
        cwd: root.join("implementations/swift"),
        timeout: Duration::from_secs(300),
    }]
}

fn assert_profile_inventory(
    profile: Profile,
    direct: &[DirectAdapter],
    native: &[NativeCheck],
) -> Result<()> {
    let required: HashSet<&str> = profile.required_kits().into_iter().collect();
    let covered: HashSet<&str> = direct
        .iter()
        .map(|a| a.kit)
        .chain(native.iter().map(|c| c.kit))
        .collect();
    let mut missing: Vec<_> = required.difference(&covered).copied().collect();
    let mut extra: Vec<_> = covered.difference(&required).copied().collect();
    missing.sort_unstable();
    extra.sort_unstable();
    if !missing.is_empty() {
        return Err(format!(
            "{} profile leaves kit(s) uncovered: {}",
            profile.name(),
            missing.join(", ")
        ));
    }
    if !extra.is_empty() {
        return Err(format!(
            "{} profile covers unexpected kit(s): {}",
            profile.name(),
            extra.join(", ")
        ));
    }
    println!("  kits: {}", profile.required_kits().join(", "));
    Ok(())
}

fn default_jobs() -> usize {
    let host = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    std::cmp::min(host, 4).max(1)
}

fn run_ordered_jobs<T: Send + 'static>(jobs: Vec<OrderedJob<T>>, max_jobs: usize) -> Vec<T> {
    if jobs.is_empty() {
        return Vec::new();
    }

    let worker_count = std::cmp::min(max_jobs.max(1), jobs.len());
    let queue: VecDeque<_> = jobs.into_iter().enumerate().collect();
    let queue = Arc::new(Mutex::new(queue));
    let (tx, rx) = mpsc::channel();
    let mut handles = Vec::with_capacity(worker_count);

    for _ in 0..worker_count {
        let queue = Arc::clone(&queue);
        let tx = tx.clone();
        handles.push(thread::spawn(move || loop {
            let next = queue
                .lock()
                .expect("ordered job queue poisoned")
                .pop_front();
            let Some((index, job)) = next else {
                break;
            };
            let result = job();
            tx.send((index, result))
                .expect("ordered job receiver dropped");
        }));
    }
    drop(tx);

    let mut results = Vec::new();
    for result in rx {
        results.push(result);
    }
    for handle in handles {
        handle.join().expect("ordered job panicked");
    }
    results.sort_by_key(|(index, _)| *index);
    results.into_iter().map(|(_, result)| result).collect()
}

fn run_one_direct_adapter(
    kit: &'static str,
    emit: fn(&str) -> Result<String>,
    fixtures: Vec<(String, Option<Fixture>)>,
) -> Vec<DirectCheckResult> {
    fixtures
        .into_iter()
        .map(|(fixture_name, fixture)| {
            let Some(fixture) = fixture else {
                return DirectCheckResult {
                    kit,
                    fixture_name: fixture_name.clone(),
                    capability: String::new(),
                    outcome: DirectCheckOutcome::MissingFixture(format!(
                        "missing conformance fixture `{fixture_name}`"
                    )),
                };
            };

            match emit(&fixture_name) {
                Ok(got) if !cid_is_well_formed(&got) => DirectCheckResult {
                    kit,
                    fixture_name,
                    capability: fixture.capability,
                    outcome: DirectCheckOutcome::MalformedCid(got),
                },
                Ok(got) if got == fixture.hash => DirectCheckResult {
                    kit,
                    fixture_name,
                    capability: fixture.capability,
                    outcome: DirectCheckOutcome::Pass,
                },
                Ok(got) => DirectCheckResult {
                    kit,
                    fixture_name,
                    capability: fixture.capability,
                    outcome: DirectCheckOutcome::CidMismatch {
                        got,
                        want: fixture.hash,
                    },
                },
                Err(e) => DirectCheckResult {
                    kit,
                    fixture_name,
                    capability: fixture.capability,
                    outcome: DirectCheckOutcome::AdapterError(e),
                },
            }
        })
        .collect()
}

fn print_direct_result(result: &DirectCheckResult) -> usize {
    match &result.outcome {
        DirectCheckOutcome::Pass => {
            println!("  PASS {} ({})", result.fixture_name, result.capability);
            0
        }
        DirectCheckOutcome::MissingFixture(e) | DirectCheckOutcome::AdapterError(e) => {
            println!("  FAIL {}: {e}", result.fixture_name);
            1
        }
        DirectCheckOutcome::MalformedCid(got) => {
            println!(
                "  FAIL {}: adapter emitted malformed CID: {got:?}",
                result.fixture_name
            );
            1
        }
        DirectCheckOutcome::CidMismatch { got, want } => {
            println!("  FAIL {}: CID mismatch", result.fixture_name);
            println!("    got:  {got}");
            println!("    want: {want}");
            1
        }
    }
}

fn run_direct_adapters(
    adapters: &[DirectAdapter],
    fixtures: &std::collections::BTreeMap<String, Fixture>,
    jobs: usize,
) -> usize {
    let mut failures = 0;
    let adapter_jobs: Vec<OrderedJob<Vec<DirectCheckResult>>> = adapters
        .iter()
        .map(|adapter| {
            let kit = adapter.kit;
            let emit = adapter.emit;
            let adapter_fixtures = adapter
                .fixtures
                .iter()
                .map(|name| ((*name).to_string(), fixtures.get(*name).cloned()))
                .collect();
            Box::new(move || run_one_direct_adapter(kit, emit, adapter_fixtures))
                as OrderedJob<Vec<DirectCheckResult>>
        })
        .collect();

    for adapter_results in run_ordered_jobs(adapter_jobs, jobs) {
        if let Some(first) = adapter_results.first() {
            println!("\n[{}] direct CID adapter", first.kit);
        }
        for result in adapter_results {
            failures += print_direct_result(&result);
        }
    }
    failures
}

fn run_native_checks(checks: &[NativeCheck], jobs: usize) -> usize {
    let mut failures = 0;
    let native_jobs: Vec<OrderedJob<NativeCheckResult>> = checks
        .iter()
        .map(|check| {
            let name = check.name;
            let cmd = check.cmd.clone();
            let cwd = check.cwd.clone();
            let timeout = check.timeout;
            Box::new(move || {
                let proc = run_cmd(&cmd, &cwd, timeout);
                NativeCheckResult { name, cmd, proc }
            }) as OrderedJob<NativeCheckResult>
        })
        .collect();

    for result in run_ordered_jobs(native_jobs, jobs) {
        println!("\n[native] {}", result.name);
        if result.proc.code == 0 {
            println!("  PASS {}", result.cmd.join(" "));
        } else {
            failures += 1;
            println!("  FAIL {}", result.cmd.join(" "));
            println!(
                "{}",
                tail(
                    &format!("{}\n{}", result.proc.stderr, result.proc.stdout),
                    4000
                )
            );
        }
    }
    failures
}

fn run_protocol_contract_gate(
    profile: Profile,
    jobs: usize,
    bootstrap_self_contract_attestations: bool,
) -> Result<usize> {
    println!("\nProtocol Contract Bootstrap Gate");
    let got = protocol_contract_set_cid()?;
    if got != EXPECTED_PROTOCOL_CONTRACT_SET_CID {
        return Err(format!(
            "protocolContractSetCid drift:\n  got:  {got}\n  want: {EXPECTED_PROTOCOL_CONTRACT_SET_CID}"
        ));
    }
    println!("  protocolContractSetCid: {got}");

    bootstrap_self_contract_toolchains(profile)?;

    let producers = self_contract_producers(profile);
    let mint_jobs: Vec<OrderedJob<(SelfContractProducer, Result<LiveSelfContractArtifact>)>> =
        producers
            .into_iter()
            .map(|producer| {
                Box::new(move || {
                    let result = mint_live_self_contract_artifact(producer);
                    (producer, result)
                })
                    as OrderedJob<(SelfContractProducer, Result<LiveSelfContractArtifact>)>
            })
            .collect();

    let mut failures = 0;
    let minted = run_ordered_jobs(mint_jobs, jobs);

    if bootstrap_self_contract_attestations {
        println!("  bootstrapping self-contract attestations from live kit artifacts");
        for (_, result) in &minted {
            if let Ok(artifact) = result {
                if let Err(e) = sign_self_contract_attestation(artifact) {
                    failures += 1;
                    println!(
                        "  FAIL {:<10} attestation bootstrap failed: {e}",
                        artifact.kit
                    );
                }
            }
        }
    }

    let attestations = load_self_contract_attestations(profile)?;
    for (producer, result) in minted {
        let attestation = attestations
            .iter()
            .find(|a| a.attestation_lang == producer.attestation_lang)
            .ok_or_else(|| format!("missing attestation for {}", producer.attestation_lang))?;
        match result.and_then(|artifact| {
            assert_live_artifact_matches_attestation(&artifact, attestation)?;
            Ok(artifact)
        }) {
            Ok(artifact) => {
                println!(
                    "  PASS {:<10} live proof {} bytes, {} members, {} contracts; selfContractSetCid pinned",
                    attestation.kit,
                    artifact.proof_bytes_len,
                    artifact.proof_member_count,
                    artifact.proof_contract_count
                );
            }
            Err(e) => {
                failures += 1;
                println!("  FAIL {:<10} {e}", attestation.kit);
            }
        }
    }
    Ok(failures)
}

fn print_help() {
    println!(
        "cross-kit-conformance\n\
         \n\
         Usage: cross-kit-conformance [--profile linux|swift|all] [--jobs N] [--bootstrap-self-contract-attestations]\n\
         \n\
         The Rust harness validates catalog-pinned fixture CIDs. Adapters may\n\
         produce any representation internally; the conformance boundary is\n\
         the protocol CID.\n\
         \n\
         --bootstrap-self-contract-attestations re-signs the pinned\n\
         .provekit/self-contracts-attestations/*.json files from live,\n\
         verifier-loadable kit-emitted proof artifacts before checking them.\n\
         \n\
         --jobs N runs selected kit/check jobs concurrently. The default is\n\
         bounded to 4 so local and CI output stay usable."
    );
}

fn parse_config<I, S>(args: I) -> Result<RunConfig>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut config = RunConfig {
        profile: Profile::default_for_host(),
        jobs: default_jobs(),
        bootstrap_self_contract_attestations: false,
    };
    let mut args = args.into_iter().map(Into::into).skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--profile" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--profile requires linux, swift, or all".to_string())?;
                config.profile = Profile::parse(&value)?;
            }
            "--jobs" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--jobs requires a positive integer".to_string())?;
                let jobs = value
                    .parse::<usize>()
                    .map_err(|_| format!("--jobs requires a positive integer, got `{value}`"))?;
                if jobs == 0 {
                    return Err("--jobs must be greater than zero".to_string());
                }
                config.jobs = jobs;
            }
            "--bootstrap-self-contract-attestations" | "--update-self-contract-attestations" => {
                config.bootstrap_self_contract_attestations = true;
            }
            "-h" | "--help" => {
                print_help();
                return Err("__help__".to_string());
            }
            other => return Err(format!("unknown argument `{other}`")),
        }
    }
    Ok(config)
}

fn run(config: RunConfig) -> Result<usize> {
    let profile = config.profile;
    let fixture_file = load_fixtures()?;
    println!("\nCatalog-pinned Cross-Kit Conformance");
    assert_catalog_pin(&fixture_file)?;
    assert_fixture_hash_pins(&fixture_file)?;
    println!(
        "  catalog: {} {}",
        fixture_file.catalog_version, fixture_file.catalog_cid
    );
    println!("  jobs: {}", config.jobs);

    let fixtures = make_fixture_map(fixture_file);
    for name in CORE_FIXTURES.iter().chain(["bridge_decl_v1_4"].iter()) {
        require_fixture(&fixtures, name)?;
    }

    let mut direct = Vec::new();
    let mut native = Vec::new();
    if matches!(profile, Profile::Linux | Profile::All) {
        direct.extend(linux_direct_adapters());
        native.extend(linux_native_checks());
    }
    if matches!(profile, Profile::Swift | Profile::All) {
        direct.extend(swift_direct_adapters());
        native.extend(swift_native_checks());
    }
    assert_profile_inventory(profile, &direct, &native)?;

    let failures = run_protocol_contract_gate(
        profile,
        config.jobs,
        config.bootstrap_self_contract_attestations,
    )? + run_direct_adapters(&direct, &fixtures, config.jobs)
        + run_native_checks(&native, config.jobs);
    println!("\nResult");
    if failures == 0 {
        println!("  all selected conformance CID checks passed");
    } else {
        println!("  {failures} conformance failure(s)");
    }
    Ok(failures)
}

pub fn main_entry<I, S>(args: I) -> ExitCode
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let config = match parse_config(args) {
        Ok(config) => config,
        Err(e) if e == "__help__" => return ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("fatal: {e}");
            return ExitCode::FAILURE;
        }
    };

    match run(config) {
        Ok(0) => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("fatal: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn critical_gate_wiring_uses_rust_harness() {
        let root = repo_root();
        let makefile = fs::read_to_string(root.join("Makefile")).expect("read Makefile");
        let ci = fs::read_to_string(root.join(".github/workflows/ci.yml")).expect("read CI");

        assert!(
            !makefile.contains("conformance/run.py"),
            "Makefile still uses the Python conformance harness"
        );
        assert!(
            !ci.contains("conformance/run.py"),
            "CI still uses the Python conformance harness"
        );
        assert!(
            makefile.contains("tools/cross-kit-conformance/Cargo.toml"),
            "Makefile must call the Rust conformance harness"
        );
        assert!(
            ci.contains("tools/cross-kit-conformance/Cargo.toml"),
            "CI must call the Rust conformance harness"
        );
    }

    #[test]
    fn fixture_pins_are_cids_not_byte_assertions() {
        let fixture_file = load_fixtures().expect("load fixtures");
        assert_fixture_hash_pins(&fixture_file).expect("fixture JCS hashes match CID pins");
        for fixture in fixture_file.fixture {
            assert!(
                cid_is_well_formed(&fixture.hash),
                "{} hash is malformed",
                fixture.name
            );
        }
    }

    #[test]
    fn profile_inventory_covers_expected_kits() {
        let mut direct = linux_direct_adapters();
        let native = linux_native_checks();
        assert_profile_inventory(Profile::Linux, &direct, &native).expect("linux inventory");
        direct.extend(swift_direct_adapters());
        let mut native_all = native;
        native_all.extend(swift_native_checks());
        assert_profile_inventory(Profile::All, &direct, &native_all).expect("all inventory");
    }

    #[test]
    fn malformed_cids_are_rejected() {
        assert!(cid_is_well_formed(
            "blake3-512:5eade72c08811b2d38adcb158eced38f3d319de090d59b2fa7a77ad830169e18539d2b75d2a2838c545e644a688cf137603674523ff37f1586a650f6dd05aeaa"
        ));
        assert!(!cid_is_well_formed("blake3-512:ABC"));
        assert!(!cid_is_well_formed("sha256:abc"));
    }

    #[test]
    fn jobs_argument_parses_and_rejects_zero() {
        let config = parse_config([
            "cross-kit-conformance",
            "--profile",
            "linux",
            "--jobs",
            "3",
            "--bootstrap-self-contract-attestations",
        ])
        .expect("parse config");
        assert_eq!(config.profile, Profile::Linux);
        assert_eq!(config.jobs, 3);
        assert!(config.bootstrap_self_contract_attestations);

        let err = parse_config(["cross-kit-conformance", "--jobs", "0"])
            .expect_err("zero jobs must be rejected");
        assert!(err.contains("greater than zero"), "got: {err}");
    }

    #[test]
    fn ordered_jobs_preserve_input_order_when_finished_out_of_order() {
        let jobs: Vec<OrderedJob<&'static str>> = vec![
            Box::new(|| {
                std::thread::sleep(Duration::from_millis(30));
                "slow-first"
            }),
            Box::new(|| "fast-second"),
        ];

        let results = run_ordered_jobs(jobs, 2);
        assert_eq!(results, vec!["slow-first", "fast-second"]);
    }

    #[test]
    fn run_cmd_drains_large_child_stderr() {
        let cmd = vec![
            "sh".to_string(),
            "-c".to_string(),
            "i=0; while [ $i -lt 20000 ]; do echo noisy >&2; i=$((i+1)); done; echo done"
                .to_string(),
        ];
        let result = run_cmd(&cmd, &repo_root(), Duration::from_secs(10));
        assert_eq!(result.code, 0, "stderr: {}", tail(&result.stderr, 400));
        assert!(result.stdout.contains("done"));
        assert!(
            result.stderr.len() > 16 * 1024,
            "test must exceed a small pipe buffer"
        );
    }

    #[test]
    fn self_contract_bootstrap_covers_linux_profile_with_real_mint_emitters() {
        let producers = self_contract_producers(Profile::Linux);
        let kits: Vec<_> = producers.iter().map(|producer| producer.kit).collect();
        assert_eq!(kits, Profile::Linux.required_kits());
        assert!(producers
            .iter()
            .all(|producer| !producer.mint_kit_alias.is_empty()));
        let targets = self_contract_bootstrap_targets(Profile::Linux);
        assert!(targets.contains(&"build-rust"));
        assert!(targets.contains(&"build-ruby"));
    }

    #[test]
    fn conformance_path_prefers_homebrew_runtime_bins_before_usr_bin() {
        let base = OsString::from(
            "/usr/bin:/bin:/usr/local/opt/ruby/bin:/usr/local/opt/openjdk/bin:/usr/local/bin",
        );
        let got = prepend_unique_path_dirs(
            Some(&base),
            &[
                PathBuf::from("/usr/local/opt/ruby/bin"),
                PathBuf::from("/usr/local/opt/openjdk/bin"),
            ],
        );
        let parts: Vec<_> = std::env::split_paths(&got).collect();

        let ruby_pos = parts
            .iter()
            .position(|p| p == Path::new("/usr/local/opt/ruby/bin"))
            .expect("ruby bin present");
        let java_pos = parts
            .iter()
            .position(|p| p == Path::new("/usr/local/opt/openjdk/bin"))
            .expect("openjdk bin present");
        let usr_pos = parts
            .iter()
            .position(|p| p == Path::new("/usr/bin"))
            .expect("/usr/bin present");

        assert!(ruby_pos < usr_pos, "ruby bin must outrank /usr/bin");
        assert!(java_pos < usr_pos, "openjdk bin must outrank /usr/bin");
        assert_eq!(
            parts
                .iter()
                .filter(|p| p == &&PathBuf::from("/usr/local/opt/ruby/bin"))
                .count(),
            1
        );
    }

    #[test]
    fn ruby_fixture_commands_run_under_bundler() {
        let cmd = ruby_bundle_exec_cmd(&["-Ilib", "-e", "puts :ok"]);
        assert_eq!(
            cmd,
            vec!["ruby", "-S", "bundle", "exec", "ruby", "-Ilib", "-e", "puts :ok"]
        );

        let native = linux_native_checks()
            .into_iter()
            .find(|check| check.kit == "ruby")
            .expect("ruby native check");
        assert_eq!(&native.cmd[..5], ["ruby", "-S", "bundle", "exec", "ruby"]);
    }

    #[test]
    fn mint_stdout_requires_bundle_and_contract_set_cids() {
        let artifact = parse_mint_stdout(
            "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\ncontractSetCid: blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\n",
        )
        .expect("parse full mint output");
        assert_eq!(artifact.bundle_cid.len(), 139);
        assert_eq!(artifact.contract_set_cid.len(), 139);

        let err = parse_mint_stdout(
            "contractSetCid: blake3-512:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\n",
        )
        .expect_err("bundle cid is required");
        assert!(err.contains("bundle CID"), "got: {err}");
    }

    #[test]
    fn live_proof_validation_rejects_hash_correct_garbage() {
        let tmp = TempDir::new("pk_garbage_proof").expect("tempdir");
        let bytes = b"not a proof envelope";
        let cid = blake3_512_of(bytes);
        fs::write(tmp.path().join(format!("{cid}.proof")), bytes).expect("write garbage proof");

        let err = validate_live_proof_artifact("rust", tmp.path(), &cid)
            .expect_err("hash-correct garbage proof must be rejected");
        assert!(err.contains("load errors"), "got: {err}");
    }

    #[test]
    fn protocol_contract_set_cid_is_pinned_to_rust_source() {
        let got = protocol_contract_set_cid().expect("derive protocol contract set CID");
        assert_eq!(got, EXPECTED_PROTOCOL_CONTRACT_SET_CID);
    }

    #[test]
    fn self_contract_attestations_pin_non_empty_contract_set_cids() {
        let attestations =
            load_self_contract_attestations(Profile::Linux).expect("load linux attestations");
        assert_eq!(attestations.len(), 11);

        for attestation in attestations {
            assert_ne!(
                attestation.contract_set_cid, EMPTY_CONTRACT_SET_CID,
                "{} self-contract set is empty",
                attestation.kit
            );
        }
    }
}
