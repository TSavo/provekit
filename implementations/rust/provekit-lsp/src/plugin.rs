// Language plugin RPC client for the ProvekIt LSP server.
//
// Mirrors the lift-plugin protocol from `provekit-cli/src/cmd_mint.rs`.
// Each language plugin is a binary that speaks NDJSON-over-stdio JSON-RPC.
//
// Plugin manifest lives at:
//   .provekit/lsp/<name>/manifest.toml   (project-local)
//   ~/.config/provekit/lsp/<name>/manifest.toml   (user-global)
//
// Manifest format:
//   name = "provekit-lsp-rust"
//   command = ["provekit-lsp-rust"]
//   # optional:
//   # working_dir = "./subproject"
//
// The main LSP server spawns the plugin with `--rpc` appended, then:
//   1. Sends `initialize` -> plugin responds with name/version/capabilities
//   2. Sends `parse`     -> plugin responds with annotations array
//   3. Sends `shutdown`  -> plugin exits cleanly

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use tower_lsp::lsp_types::{Position, Range};

use crate::parser::{Annotation, AnnotationKind, SourceAnnotations};

/// A spawned language plugin process.
pub struct LanguagePlugin {
    name: String,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    _child: Child,
    next_id: i64,
}

impl std::fmt::Debug for LanguagePlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LanguagePlugin")
            .field("name", &self.name)
            .field("next_id", &self.next_id)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Default)]
struct PluginManifest {
    name: String,
    command: Vec<String>,
    working_dir: Option<PathBuf>,
}

/// Parse a plugin manifest file.
fn parse_manifest(path: &Path) -> Result<PluginManifest, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("read {}: {e}", path.display()))?;
    let mut m = PluginManifest::default();
    for line in text.lines() {
        let line = match line.find('#') {
            Some(p) => &line[..p],
            None => line,
        }
        .trim();
        if line.is_empty() || line.starts_with('[') {
            continue;
        }
        let Some(eq) = line.find('=') else { continue };
        let key = line[..eq].trim();
        let val = line[eq + 1..].trim();
        match key {
            "name" => m.name = val.trim_matches('"').to_string(),
            "working_dir" => m.working_dir = Some(PathBuf::from(val.trim_matches('"'))),
            "command" => {
                let inner = val.trim_matches(|c| c == '[' || c == ']');
                m.command = inner
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            _ => {}
        }
    }
    if m.command.is_empty() {
        return Err(format!("manifest {} has no `command`", path.display()));
    }
    Ok(m)
}

/// Find a plugin manifest by name.
fn find_manifest(project_root: &Path, name: &str) -> Result<PluginManifest, String> {
    let project_local = project_root
        .join(".provekit")
        .join("lsp")
        .join(name)
        .join("manifest.toml");
    if project_local.exists() {
        return parse_manifest(&project_local);
    }
    if let Some(home) = std::env::var_os("HOME") {
        let user_global = PathBuf::from(home)
            .join(".config")
            .join("provekit")
            .join("lsp")
            .join(name)
            .join("manifest.toml");
        if user_global.exists() {
            return parse_manifest(&user_global);
        }
    }
    Err(format!(
        "no plugin manifest for lsp language `{name}` (looked in .provekit/lsp/{name}/manifest.toml and ~/.config/provekit/lsp/{name}/manifest.toml)"
    ))
}

impl LanguagePlugin {
    /// Spawn a language plugin by manifest name and send initialize.
    pub fn spawn_by_name(
        project_root: &Path,
        name: &str,
    ) -> Result<Self, String> {
        let manifest = find_manifest(project_root, name)?;
        Self::spawn(manifest, project_root)
    }

    /// Spawn a language plugin directly from a command array.
    pub fn spawn_direct(
        command: &[String],
        args: &[String],
        project_root: &Path,
    ) -> Result<Self, String> {
        let manifest = PluginManifest {
            name: command.first().cloned().unwrap_or_default(),
            command: command.to_vec(),
            working_dir: None,
        };
        let plugin = Self::spawn(manifest, project_root)?;
        // Append extra args
        // (spawn already appended --rpc; these are additional)
        // We don't have a way to send args post-spawn, so we need to rebuild.
        // Actually spawn_manifest handles this. Let me refactor.
        // For now direct spawn uses the command as-is.
        drop(plugin);
        // Re-spawn with extra args
        let mut full_cmd = command.to_vec();
        full_cmd.push("--rpc".to_string());
        full_cmd.extend_from_slice(args);
        let manifest = PluginManifest {
            name: command.first().cloned().unwrap_or_default(),
            command: full_cmd,
            working_dir: None,
        };
        Self::spawn(manifest, project_root)
    }

    fn spawn(manifest: PluginManifest, project_root: &Path) -> Result<Self, String> {
        let mut cmd = Command::new(&manifest.command[0]);
        if manifest.command.len() > 1 {
            cmd.args(&manifest.command[1..]);
        }
        cmd.arg("--rpc");
        if let Some(wd) = &manifest.working_dir {
            let resolved = if wd.is_absolute() {
                wd.clone()
            } else {
                project_root.join(wd)
            };
            cmd.current_dir(resolved);
        }
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::inherit());

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("spawn {:?}: {e}", manifest.command))?;
        let stdin = child
            .stdin
            .take()
            .ok_or("no stdin")?;
        let stdout = child
            .stdout
            .take()
            .ok_or("no stdout")?;

        let mut plugin = LanguagePlugin {
            name: manifest.name,
            stdin,
            stdout: BufReader::new(stdout),
            _child: child,
            next_id: 1,
        };

        plugin.handshake()?;
        Ok(plugin)
    }

    fn handshake(&mut self) -> Result<(), String> {
        let req = json!({
            "jsonrpc": "2.0",
            "id": self.next_id(),
            "method": "initialize",
            "params": {
                "client": {"name": "provekit-lsp", "version": env!("CARGO_PKG_VERSION")},
                "protocol_version": "provekit-lsp-plugin/1",
            }
        });
        let resp = self.exchange(&req)?;
        if resp.get("error").is_some() {
            return Err(format!("plugin `{}` initialize failed: {resp}", self.name));
        }
        Ok(())
    }

    /// Parse a file and return annotations.
    pub fn parse(&mut self, uri: &str, text: &str) -> Result<SourceAnnotations, String> {
        let req = json!({
            "jsonrpc": "2.0",
            "id": self.next_id(),
            "method": "parse",
            "params": {
                "uri": uri,
                "text": text,
            }
        });
        let resp = self.exchange(&req)?;
        if let Some(err) = resp.get("error") {
            return Err(format!("plugin `{}` parse error: {err}", self.name));
        }
        let result = resp
            .get("result")
            .cloned()
            .ok_or("parse response missing result")?;
        parse_plugin_annotations(&result)
    }

    /// Shut down the plugin gracefully.
    pub fn shutdown(&mut self) -> Result<(), String> {
        let req = json!({
            "jsonrpc": "2.0",
            "id": self.next_id(),
            "method": "shutdown"
        });
        let _ = self.exchange(&req);
        let _ = self.stdin.flush();
        // Close stdin to signal EOF to child
        // We can't close ChildStdin directly, but dropping works when we own it.
        // For &mut self, we just let the process exit naturally.
        let _ = self._child.try_wait();
        Ok(())
    }

    fn exchange(&mut self, req: &Value) -> Result<Value, String> {
        let line = serde_json::to_string(req)
            .map_err(|e| format!("encode: {e}"))?;
        writeln!(self.stdin, "{line}")
            .map_err(|e| format!("write: {e}"))?;
        self.stdin
            .flush()
            .map_err(|e| format!("flush: {e}"))?;

        let mut buf = String::new();
        let n = self
            .stdout
            .read_line(&mut buf)
            .map_err(|e| format!("read: {e}"))?;
        if n == 0 {
            return Err("plugin closed stdout".to_string());
        }
        let v: Value = serde_json::from_str(&buf)
            .map_err(|e| format!("decode: {e}\n  raw: {buf}"))?;
        Ok(v)
    }

    fn next_id(&mut self) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

/// Parse annotations from a plugin's JSON-RPC response.
fn parse_plugin_annotations(value: &Value) -> Result<SourceAnnotations, String> {
    let arr = value
        .get("annotations")
        .and_then(|v| v.as_array())
        .ok_or("expected `annotations` array in plugin response")?;

    let mut annotations = Vec::new();
    for item in arr {
        let function_name = item
            .get("function_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let kind = item
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let target_cid = item
            .get("target_cid")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let range = parse_range(item.get("range"));

        let kind = match kind {
            "implement" => AnnotationKind::Implement {
                target_cid: target_cid.clone().unwrap_or_default(),
            },
            "contract" => AnnotationKind::Contract,
            "verify" => AnnotationKind::Verify,
            _ => continue, // skip unknown kinds
        };

        annotations.push(Annotation {
            function_name,
            kind,
            target_cid,
            range,
        });
    }

    Ok(SourceAnnotations { annotations })
}

fn parse_range(value: Option<&Value>) -> Range {
    let default = Range {
        start: Position { line: 0, character: 0 },
        end: Position { line: 0, character: 0 },
    };
    let Some(v) = value else { return default };
    let start = v.get("start").and_then(parse_position).unwrap_or(Position { line: 0, character: 0 });
    let end = v.get("end").and_then(parse_position).unwrap_or(Position { line: 0, character: 0 });
    Range { start, end }
}

fn parse_position(value: &Value) -> Option<Position> {
    let line = value.get("line")?.as_u64()? as u32;
    let character = value.get("character")?.as_u64()? as u32;
    Some(Position { line, character })
}

/// Convenience: try to load a plugin for a language config.
pub fn load_plugin(
    project_root: &Path,
    lang_config: &crate::config::LanguagePluginConfig,
) -> Result<LanguagePlugin, String> {
    if let Some(plugin_name) = &lang_config.plugin {
        // Try manifest lookup first
        if !plugin_name.contains('/') && !plugin_name.contains("\\") {
            LanguagePlugin::spawn_by_name(project_root, plugin_name)
        } else {
            // Direct path or binary name
            let cmd = vec![plugin_name.clone()];
            LanguagePlugin::spawn_direct(&cmd, &lang_config.plugin_args, project_root)
        }
    } else {
        Err(format!(
            "language `{}` has no plugin configured",
            lang_config.name
        ))
    }
}
