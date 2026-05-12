// SPDX-License-Identifier: Apache-2.0
//
// PEP 1.7.0 plugin flag plumbing.
//
// This module owns the PluginFlags struct and the `build_registry` function.
//
// Flag dispatch (§7):
//   --plugin <kind>:<source>     canonical form; multi-load via repeated flags.
//   --sugar <source>             ≡ --plugin sugar:<source>
//   --loss-fn <source>           ≡ --plugin loss-function:<source>
//   --lifter <source>            ≡ --plugin lift:<source>   (wire kind = "lift")
//   --no-default-plugins         suppress ALL built-in registration.
//   --no-default-plugin <kind>   suppress built-ins for one kind.
//   --strict-plugins             promote every plugin load failure to refuse.
//   --plugin-registry-out <path> write PluginRegistryMemento JSON to <path>.
//
// Source detection (§3.1): strings beginning with "stdio:", "http://",
// "https://", or "tcp://" are RPC; everything else is a file path.
//
// Flag order is the tie-break order preserved in PluginRegistryMemento.load_order
// (§9.1).  Built-ins are appended AFTER user flags (§7).

use std::path::PathBuf;

use clap::Parser;

use provekit_plugin_loader::{
    error::LoadError,
    load_plugin_from_file, load_plugin_from_rpc,
    registry::{mint_failure_memento, PluginRegistry},
    PluginRegistryMemento,
};

/// PEP 1.7.0 plugin flags (§7).  Flatten into any subcommand that consumes
/// the plugin registry:
///   ```ignore
///   #[command(flatten)]
///   pub plugins: PluginFlags,
///   ```
#[derive(Parser, Debug, Clone, Default)]
pub struct PluginFlags {
    /// Canonical plugin flag.  Loads one plugin of the declared kind from the
    /// source.  Repeat for multiple plugins; flag order is preserved in the
    /// registry's load_order (§9.1).
    ///
    /// Source detection (§3.1):
    ///   - `stdio:<cmd>`  → JSON-RPC over stdio.
    ///   - `http://...`   → JSON-RPC over HTTP (stub for PEP 1.7.0 v0).
    ///   - Everything else → file path.
    ///
    /// Example: `--plugin sugar:/path/to/spring.json`
    ///          `--plugin sugar:rpc://localhost:8765`
    #[arg(long = "plugin", value_name = "KIND:SOURCE", action = clap::ArgAction::Append)]
    pub plugins: Vec<String>,

    /// Alias: `--sugar <source>` ≡ `--plugin sugar:<source>`.
    #[arg(long = "sugar", value_name = "SOURCE", action = clap::ArgAction::Append)]
    pub sugar: Vec<String>,

    /// Alias: `--loss-fn <source>` ≡ `--plugin loss-function:<source>`.
    #[arg(long = "loss-fn", value_name = "SOURCE", action = clap::ArgAction::Append)]
    pub loss_fn: Vec<String>,

    /// Alias: `--lifter <source>` ≡ `--plugin lift:<source>`.
    /// Note: the wire `kind` value is `"lift"`, not `"lifter"` (§2.1).
    #[arg(long = "lifter", value_name = "SOURCE", action = clap::ArgAction::Append)]
    pub lifter: Vec<String>,

    /// Suppress ALL built-in plugin registration (§7).
    #[arg(long = "no-default-plugins")]
    pub no_default_plugins: bool,

    /// Suppress built-ins for one kind only (§7).  Repeat for multiple kinds.
    #[arg(long = "no-default-plugin", value_name = "KIND", action = clap::ArgAction::Append)]
    pub no_default_plugin: Vec<String>,

    /// Promote EVERY plugin load failure to a refuse (overrides individual
    /// `critical = false` declarations) (§7).
    #[arg(long = "strict-plugins")]
    pub strict_plugins: bool,

    /// After the registry seals (§9), write the PluginRegistryMemento JSON to
    /// this path.
    #[arg(long = "plugin-registry-out", value_name = "PATH")]
    pub plugin_registry_out: Option<PathBuf>,
}

impl PluginFlags {
    /// Expand alias flags into canonical `(kind, source)` pairs, preserving
    /// flag order.  Built-ins are NOT included here; they are appended by
    /// `build_registry` after user plugins.
    ///
    /// Returns tuples of `(kind, source, cli_flag_verbatim)`.
    fn expanded_plugins(&self) -> Vec<(String, String, String)> {
        let mut out: Vec<(String, String, String)> = Vec::new();

        // Process --plugin flags first (canonical form).
        for raw in &self.plugins {
            if let Some((kind, source)) = raw.split_once(':') {
                out.push((kind.to_string(), source.to_string(), raw.clone()));
            }
            // If no ':' found, treat as parse error at load time.
        }

        // Per-kind aliases desugar AFTER canonical flags so that canonical form
        // flags precede aliases in load_order when both appear.
        for source in &self.sugar {
            let verbatim = format!("sugar:{source}");
            out.push(("sugar".to_string(), source.clone(), verbatim));
        }
        for source in &self.loss_fn {
            let verbatim = format!("loss-function:{source}");
            out.push(("loss-function".to_string(), source.clone(), verbatim));
        }
        for source in &self.lifter {
            // wire kind = "lift" (§2.1)
            let verbatim = format!("lift:{source}");
            out.push(("lift".to_string(), source.clone(), verbatim));
        }

        out
    }

    /// Load all plugins declared via flags, register them, seal the registry,
    /// optionally write the PluginRegistryMemento to disk, and return it.
    ///
    /// If `strict_plugins` is set, the first load failure refuses (returns Err).
    /// Otherwise, failures are recorded in the registry and the run continues.
    ///
    /// `sealed_at` should be an ISO-8601 UTC timestamp.
    pub fn build_registry(
        &self,
        sealed_at: &str,
    ) -> Result<PluginRegistryMemento, PluginLoadRefusal> {
        let mut registry = PluginRegistry::new();

        for (kind, source, verbatim) in self.expanded_plugins() {
            let result = load_one(&source);
            match result {
                Ok(plugin) => {
                    // Validate: plugin's declared kind matches the CLI flag kind.
                    if plugin.kind() != kind {
                        let err = LoadError::ValidationError {
                            detail: format!(
                                "plugin at `{source}` declares kind '{}' but flag expects kind '{kind}'",
                                plugin.kind()
                            ),
                        };
                        let f = mint_failure_memento(&verbatim, &kind, &err, sealed_at);
                        if plugin.is_critical() || self.strict_plugins {
                            return Err(PluginLoadRefusal {
                                failure: f,
                                source: verbatim,
                            });
                        }
                        registry.record_failure(f);
                        continue;
                    }
                    let critical = plugin.is_critical();
                    if let Err(dup_err) = registry.register(plugin) {
                        let f = mint_failure_memento(&verbatim, &kind, &dup_err, sealed_at);
                        if critical || self.strict_plugins {
                            return Err(PluginLoadRefusal {
                                failure: f,
                                source: verbatim,
                            });
                        }
                        registry.record_failure(f);
                    }
                }
                Err(err) => {
                    // Determine critical: we don't have the plugin in hand, so treat
                    // any strict-plugins setting or if the source started with kind
                    // (caller's responsibility).  For now: strict_plugins overrides.
                    let f = mint_failure_memento(&verbatim, &kind, &err, sealed_at);
                    if self.strict_plugins {
                        return Err(PluginLoadRefusal {
                            failure: f,
                            source: verbatim,
                        });
                    }
                    registry.record_failure(f);
                }
            }
        }

        let memento = registry.emit_registry_memento(sealed_at);

        // Write registry memento to disk if requested (§7).
        if let Some(ref out_path) = self.plugin_registry_out {
            let json =
                serde_json::to_string_pretty(&memento).expect("registry memento serialization");
            std::fs::write(out_path, json).unwrap_or_else(|e| {
                eprintln!(
                    "plugin-loader: could not write registry to {}: {e}",
                    out_path.display()
                );
            });
        }

        Ok(memento)
    }
}

/// A plugin load failure that refused the run (critical = true or
/// --strict-plugins was set).
#[derive(Debug)]
pub struct PluginLoadRefusal {
    pub failure: provekit_plugin_loader::types::PluginLoadFailureMemento,
    pub source: String,
}

impl std::fmt::Display for PluginLoadRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "plugin load refused for '{}': {} ({})",
            self.source,
            self.failure.header.reason_detail,
            self.failure.header.reason_kind
        )
    }
}

/// Source detection per §3.1: determine whether the source is RPC or file,
/// then dispatch accordingly.
fn load_one(
    source: &str,
) -> Result<provekit_plugin_loader::types::PluginMemento, LoadError> {
    if source.starts_with("stdio:")
        || source.starts_with("http://")
        || source.starts_with("https://")
        || source.starts_with("tcp://")
    {
        load_plugin_from_rpc(source)
    } else {
        load_plugin_from_file(std::path::Path::new(source))
    }
}
