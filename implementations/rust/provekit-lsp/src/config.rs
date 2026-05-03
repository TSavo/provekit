// Configuration for the ProvekIt LSP server.
//
// Reads `.provekit/config.toml` at workspace root. Example:
//
//   [server]
//   backend = "provekit"
//   backend_args = ["verify", "--format", "json"]
//   timeout_ms = 5000
//   cache_dir = ".provekit/cache"
//
//   [[language]]
//   name = "rust"
//   extensions = [".rs"]
//   parser = "builtin:rust"
//
//   [[language]]
//   name = "go"
//   extensions = [".go"]
//   plugin = "provekit-lsp-go"
//   plugin_args = ["--rpc"]
//
// Built-in parsers are compiled into the main binary.
// External plugins are spawned as child processes and spoken to via JSON-RPC.

use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct LspConfig {
    #[serde(default = "default_server")]
    pub server: ServerConfig,
    #[serde(default)]
    pub language: Vec<LanguagePluginConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_backend")]
    pub backend: String,
    #[serde(default)]
    pub backend_args: Vec<String>,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(default = "default_cache_dir")]
    pub cache_dir: String,
    /// Optional path to the provekit-linkerd Unix domain socket.
    ///
    /// When set, `did_open` / `did_change` route through the daemon instead
    /// of the per-plugin subprocess mode.  The value may be overridden by
    /// the `--daemon-socket <path>` CLI flag.
    ///
    /// Example config.toml:
    ///   [server]
    ///   daemon_socket = "/run/user/1000/provekit/linkerd-<projectCid>.sock"
    #[serde(default)]
    pub daemon_socket: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LanguagePluginConfig {
    pub name: String,
    #[serde(default)]
    pub extensions: Vec<String>,
    /// Built-in parser identifier, e.g. "builtin:rust"
    pub parser: Option<String>,
    /// External plugin binary path or name (looked up in PATH)
    pub plugin: Option<String>,
    #[serde(default)]
    pub plugin_args: Vec<String>,
}

impl LspConfig {
    /// Build a map from file extension -> language plugin config.
    pub fn extension_map(&self) -> HashMap<String, LanguagePluginConfig> {
        let mut map = HashMap::new();
        for lang in &self.language {
            for ext in &lang.extensions {
                let ext_normalized = if ext.starts_with('.') {
                    ext.to_string()
                } else {
                    format!(".{}", ext)
                };
                map.insert(ext_normalized, lang.clone());
            }
        }
        map
    }

    /// Find the language config for a given file path.
    pub fn for_path(&self, path: &Path) -> Option<&LanguagePluginConfig> {
        let ext = path.extension()?.to_str()?;
        let with_dot = format!(".{}", ext);
        self.language.iter().find(|l| {
            l.extensions.iter().any(|e| {
                let e = if e.starts_with('.') { e.clone() } else { format!(".{}", e) };
                e == with_dot
            })
        })
    }
}

impl Default for LspConfig {
    fn default() -> Self {
        Self {
            server: default_server(),
            language: default_languages(),
        }
    }
}

fn default_server() -> ServerConfig {
    ServerConfig {
        backend: default_backend(),
        backend_args: Vec::new(),
        timeout_ms: default_timeout(),
        cache_dir: default_cache_dir(),
        daemon_socket: None,
    }
}

fn default_backend() -> String {
    "provekit".to_string()
}

fn default_timeout() -> u64 {
    5000
}

fn default_cache_dir() -> String {
    ".provekit/cache".to_string()
}

fn default_languages() -> Vec<LanguagePluginConfig> {
    vec![LanguagePluginConfig {
        name: "rust".to_string(),
        extensions: vec![".rs".to_string()],
        parser: Some("builtin:rust".to_string()),
        plugin: None,
        plugin_args: Vec::new(),
    }]
}

pub fn load_config(path: impl AsRef<Path>) -> Result<LspConfig, String> {
    let path = path.as_ref();
    if !path.exists() {
        return Ok(LspConfig::default());
    }

    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("read config: {}", e))?;

    let config: LspConfig = toml::from_str(&text)
        .map_err(|e| format!("parse config: {}", e))?;

    Ok(config)
}
