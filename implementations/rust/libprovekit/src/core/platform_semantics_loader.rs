// SPDX-License-Identifier: Apache-2.0
//
// Load each language kit's PlatformSemanticsDeclaration via JSON-RPC.
//
// Per #1270: the kit IS the authority on its platform semantics. libprovekit
// MUST NOT carry a hardcoded Rust mirror of any kit's declaration. This
// loader spawns the kit binary as a JSON-RPC subprocess (PEP 1.7.0 plugin
// protocol), calls `provekit.plugin.platform_semantics`, and parses the
// response into a `PlatformSemanticsDeclaration`.
//
// The loader is the substrate-uniform replacement for the hardcoded
// `*_kit_declaration()` calls that previously lived in
// `libprovekit/src/core/platform_semantics/{typescript,java,python_*}.rs`
// and the in-tree includes of provekit-realize-rust-core +
// provekit-realize-c-core platform_semantics.rs files.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use serde_json::{json, Value};

use crate::core::types::PlatformSemanticsDeclaration;

#[derive(Debug, thiserror::Error)]
pub enum PlatformSemanticsLoadError {
    #[error("kit binary {binary} not found on PATH")]
    MissingBinary { binary: String },
    #[error("kit RPC failed: {0}")]
    Failed(String),
    #[error("kit RPC response missing 'result' field: {0}")]
    MissingResult(String),
    #[error("kit RPC result did not parse as PlatformSemanticsDeclaration: {0}")]
    ResultShape(String),
}

#[derive(Debug, Clone)]
struct ResolvedPlatformSemanticsCommand {
    argv: Vec<String>,
    working_dir: Option<PathBuf>,
}

/// Resolve kit commands by project manifest, workspace build convention, then
/// PATH convention. The substrate does NOT enumerate kits.
///
/// Convention:
///   1. Try `.provekit/realize/<kit_id>/manifest.toml` from the current repo
///   2. Try workspace-local build products under `implementations/<kit_id>/`
///   3. Try `provekit-realize-<kit_id>-core` on PATH (pure language-kit form)
///   4. Try `provekit-realize-<kit_id>` on PATH (full kit-identity form,
///      e.g., `typescript-pg`)
///
/// If neither candidate is available, returns the language-kit form as a
/// best-effort command so
/// the loader's spawn produces a MissingBinary error citing the
/// convention name, rather than collapsing two failure modes (unknown
/// kit vs binary-missing) into the same None.
///
/// Adding a new kit requires nothing here: name the binary by convention
/// or register a manifest and the substrate finds it. The previous hardcoded
/// match table was substrate-uniform-pattern violation: kit enumeration inside
/// libprovekit.
fn kit_command_candidates(kit_id: &str) -> Vec<ResolvedPlatformSemanticsCommand> {
    let mut candidates = Vec::new();
    if let Some(root) = find_workspace_root() {
        if let Some(command) = manifest_command_for(&root, kit_id) {
            candidates.push(command);
        }
        candidates.extend(workspace_build_candidates(&root, kit_id));
    }

    let lang_form = format!("provekit-realize-{kit_id}-core");
    let identity_form = format!("provekit-realize-{kit_id}");
    if which_on_path(&lang_form).is_some() {
        candidates.push(ResolvedPlatformSemanticsCommand {
            argv: vec![lang_form.clone()],
            working_dir: None,
        });
    }
    if which_on_path(&identity_form).is_some() {
        candidates.push(ResolvedPlatformSemanticsCommand {
            argv: vec![identity_form],
            working_dir: None,
        });
    }
    if candidates.is_empty() {
        // Best-effort: surface the language-kit-form name in MissingBinary.
        candidates.push(ResolvedPlatformSemanticsCommand {
            argv: vec![lang_form],
            working_dir: None,
        });
    }
    candidates
}

fn find_workspace_root() -> Option<PathBuf> {
    let mut current = std::env::current_dir().ok()?;
    loop {
        if current.join(".provekit").join("realize").is_dir() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

fn manifest_command_for(root: &Path, kit_id: &str) -> Option<ResolvedPlatformSemanticsCommand> {
    let manifest = root
        .join(".provekit")
        .join("realize")
        .join(kit_id)
        .join("manifest.toml");
    if !manifest.exists() {
        return None;
    }
    parse_manifest_command(&manifest, root).ok()
}

fn parse_manifest_command(
    path: &Path,
    root: &Path,
) -> Result<ResolvedPlatformSemanticsCommand, PlatformSemanticsLoadError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| PlatformSemanticsLoadError::Failed(format!("read {}: {e}", path.display())))?;
    let mut command = Vec::new();
    let mut working_dir = None;
    let mut section = String::new();
    for line in text.lines() {
        let line = match line.find('#') {
            Some(pos) => &line[..pos],
            None => line,
        }
        .trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line
                .trim_start_matches('[')
                .trim_end_matches(']')
                .trim()
                .to_string();
            continue;
        }
        let Some(eq) = line.find('=') else { continue };
        let key = line[..eq].trim();
        let value = line[eq + 1..].trim();
        match (section.as_str(), key) {
            ("", "command") => command = parse_toml_string_array(value),
            ("", "working_dir") => {
                let path = PathBuf::from(value.trim_matches('"'));
                working_dir = Some(if path.is_absolute() {
                    path
                } else {
                    root.join(path)
                });
            }
            _ => {}
        }
    }
    if command.is_empty() {
        return Err(PlatformSemanticsLoadError::Failed(format!(
            "manifest {} has no `command`",
            path.display()
        )));
    }
    Ok(ResolvedPlatformSemanticsCommand {
        argv: command,
        working_dir: working_dir.or_else(|| Some(root.to_path_buf())),
    })
}

fn parse_toml_string_array(value: &str) -> Vec<String> {
    let inner = value.trim().trim_matches(|c| c == '[' || c == ']');
    inner
        .split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn workspace_build_candidates(root: &Path, kit_id: &str) -> Vec<ResolvedPlatformSemanticsCommand> {
    let impl_dir = root.join("implementations").join(kit_id);
    let mut out = Vec::new();

    let jar = impl_dir
        .join(format!("provekit-realize-{kit_id}-core"))
        .join("target")
        .join(format!("provekit-realize-{kit_id}.jar"));
    if jar.exists() {
        out.push(ResolvedPlatformSemanticsCommand {
            argv: vec![
                "java".to_string(),
                "-jar".to_string(),
                jar.display().to_string(),
                "--rpc".to_string(),
            ],
            working_dir: Some(root.to_path_buf()),
        });
    }

    for profile in ["release", "debug"] {
        let binary = impl_dir
            .join("target")
            .join(profile)
            .join(format!("provekit-realize-{kit_id}"));
        if binary.exists() {
            out.push(ResolvedPlatformSemanticsCommand {
                argv: vec![binary.display().to_string(), "--rpc".to_string()],
                working_dir: Some(root.to_path_buf()),
            });
        }
    }

    out
}

fn which_on_path(bin: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).find_map(|dir| {
            let candidate = dir.join(bin);
            if candidate.is_file() {
                Some(candidate)
            } else {
                None
            }
        })
    })
}

/// Load a kit's PlatformSemanticsDeclaration over JSON-RPC.
///
/// Spawns the kit binary, performs the PEP 1.7.0 initialize handshake, calls
/// `provekit.plugin.platform_semantics`, parses the result, and shuts the
/// subprocess down.
pub fn load_platform_semantics(
    kit_id: &str,
) -> Result<PlatformSemanticsDeclaration, PlatformSemanticsLoadError> {
    let mut last_error = None;
    for command in kit_command_candidates(kit_id) {
        match load_platform_semantics_for_command(&command.argv, command.working_dir.as_ref()) {
            Ok(declaration) => return Ok(declaration),
            Err(error) => last_error = Some(error),
        }
    }
    Err(
        last_error.unwrap_or_else(|| PlatformSemanticsLoadError::MissingBinary {
            binary: format!("provekit-realize-{kit_id}-core"),
        }),
    )
}

/// Load using an explicit binary command. The substrate's preferred API for
/// callers that already know the kit's binary path (e.g., the CLI resolved
/// it via `.provekit/realize/<kit>/manifest.toml`).
pub fn load_platform_semantics_with_command(
    command: &[String],
    working_dir: Option<&PathBuf>,
) -> Result<PlatformSemanticsDeclaration, PlatformSemanticsLoadError> {
    load_platform_semantics_for_command(command, working_dir)
}

/// Same as `load_platform_semantics`, but caches the result per-target for
/// the lifetime of the process. Callers that load the same target many times
/// per run (the dispatcher does) should prefer this.
pub fn load_platform_semantics_cached(
    target: &str,
) -> Result<PlatformSemanticsDeclaration, PlatformSemanticsLoadError> {
    use std::collections::HashMap;
    use std::sync::Mutex;
    static CACHE: OnceLock<Mutex<HashMap<String, PlatformSemanticsDeclaration>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(map) = cache.lock() {
        if let Some(decl) = map.get(target) {
            return Ok(decl.clone());
        }
    }
    let decl = load_platform_semantics(target)?;
    if let Ok(mut map) = cache.lock() {
        map.insert(target.to_string(), decl.clone());
    }
    Ok(decl)
}

fn load_platform_semantics_for_command(
    command: &[String],
    working_dir: Option<&PathBuf>,
) -> Result<PlatformSemanticsDeclaration, PlatformSemanticsLoadError> {
    if command.is_empty() {
        return Err(PlatformSemanticsLoadError::Failed(
            "platform-semantics kit command is empty".into(),
        ));
    }

    let mut cmd = Command::new(&command[0]);
    if command.len() > 1 {
        cmd.args(&command[1..]);
    }
    if !command.iter().any(|arg| arg == "--rpc") {
        cmd.arg("--rpc");
    }
    if let Some(working_dir) = working_dir {
        cmd.current_dir(working_dir);
    }
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::inherit());

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(PlatformSemanticsLoadError::MissingBinary {
                binary: command[0].clone(),
            });
        }
        Err(error) => {
            return Err(PlatformSemanticsLoadError::Failed(format!(
                "spawn {:?}: {error}",
                command
            )));
        }
    };

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| PlatformSemanticsLoadError::Failed("stdin unavailable".into()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| PlatformSemanticsLoadError::Failed("stdout unavailable".into()))?;
    let mut reader = BufReader::new(stdout);

    let init_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "client": {"name": "libprovekit-platform-semantics-loader", "version": env!("CARGO_PKG_VERSION")},
            "protocol_version": "pep/1.7.0"
        }
    });
    writeln!(stdin, "{init_req}")
        .map_err(|e| PlatformSemanticsLoadError::Failed(format!("write initialize: {e}")))?;
    match read_response(&mut reader, 1) {
        Ok(_) => {}
        Err(error) if is_optional_handshake_error(&error) => {}
        Err(error) => return Err(error),
    }

    let req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "provekit.plugin.platform_semantics",
        "params": {}
    });
    writeln!(stdin, "{req}").map_err(|e| {
        PlatformSemanticsLoadError::Failed(format!("write platform_semantics request: {e}"))
    })?;
    let response = read_response(&mut reader, 2)?;

    let shutdown_req = json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "provekit.plugin.shutdown"
    });
    let _ = writeln!(stdin, "{shutdown_req}");
    drop(stdin);
    let _ = child.wait();

    let result = response
        .get("result")
        .cloned()
        .ok_or_else(|| PlatformSemanticsLoadError::MissingResult(response.to_string()))?;
    serde_json::from_value::<PlatformSemanticsDeclaration>(result)
        .map_err(|e| PlatformSemanticsLoadError::ResultShape(e.to_string()))
}

fn read_response<R: BufRead>(
    reader: &mut R,
    expected_id: i64,
) -> Result<Value, PlatformSemanticsLoadError> {
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|e| PlatformSemanticsLoadError::Failed(format!("read response: {e}")))?;
    if line.trim().is_empty() {
        return Err(PlatformSemanticsLoadError::Failed(
            "empty response line".into(),
        ));
    }
    let value: Value = serde_json::from_str(line.trim())
        .map_err(|e| PlatformSemanticsLoadError::Failed(format!("parse response: {e}")))?;
    let id = value.get("id").and_then(Value::as_i64).unwrap_or(-1);
    if id != expected_id {
        return Err(PlatformSemanticsLoadError::Failed(format!(
            "response id mismatch: expected {expected_id}, got {id}"
        )));
    }
    if let Some(error) = value.get("error") {
        return Err(PlatformSemanticsLoadError::Failed(format!(
            "kit returned error: {error}"
        )));
    }
    Ok(value)
}

fn is_optional_handshake_error(error: &PlatformSemanticsLoadError) -> bool {
    matches!(error, PlatformSemanticsLoadError::Failed(message)
        if message.contains("\"code\":-32601") || message.contains("METHOD_NOT_FOUND"))
}
