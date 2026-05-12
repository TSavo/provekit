// SPDX-License-Identifier: Apache-2.0
//
// §9 Registry semantics.
//
// PluginRegistry — in-memory store indexed by (kind, cid).
//
// §9.1 — PluginRegistryMemento sealed after all --plugin flags processed.
// §9.2 — Duplicate-CID collision rule: second registration of (kind, cid)
//         is silently deduplicated UNLESS the content differs (which can't
//         happen with content-addressing: same CID implies same content,
//         §6.2), so duplicate is a no-op.
// §9.3 — Registry CID computed over JCS(header_without_cid).
// §9.4 — Every output's provenance MUST cite the registry CID.
//
// Built-in plugins (when not suppressed) are appended AT THE END of the
// `load_order` array per §7.  This crate ships no built-ins; the
// PluginRegistry API accepts them via `register_builtin`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::cid::compute_registry_cid;
use crate::error::LoadError;
use crate::types::{
    PluginEnvelope, PluginLoadFailureMemento, PluginLoadFailureMementoHeader,
    PluginMemento,
};

// ---------------------------------------------------------------------------
// §9.1 PluginRegistryMemento header
// ---------------------------------------------------------------------------

/// The header of a `PluginRegistryMemento` (§9.1).
///
/// JCS key order: built_in_count, cid, failures, kind, load_order,
///                loaded, runtime_protocol_versions, schemaVersion, sealed_at
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginRegistryMementoHeader {
    pub built_in_count: usize,
    pub cid: String,
    /// CIDs of PluginLoadFailureMementos minted during this run.
    pub failures: Vec<String>,
    pub kind: String,
    /// Load order: CIDs in flag-order (user flags first, built-ins last per §7).
    pub load_order: Vec<String>,
    /// CIDs of successfully loaded plugins.
    pub loaded: Vec<String>,
    pub runtime_protocol_versions: Vec<String>,
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    pub sealed_at: String,
}

/// The sealed registry memento (§9.1).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginRegistryMemento {
    pub envelope: PluginEnvelope,
    pub header: PluginRegistryMementoHeader,
}

impl PluginRegistryMemento {
    /// The registry's own CID (§9.3).
    pub fn cid(&self) -> &str {
        &self.header.cid
    }
}

// ---------------------------------------------------------------------------
// In-memory registry
// ---------------------------------------------------------------------------

/// Composite key for the registry index: (kind, cid).
type RegistryKey = (String, String);

/// In-memory plugin registry (§9).
///
/// Call `register` for each loaded plugin.
/// Call `emit_registry_memento` once all flags are processed and built-ins
/// are appended, to seal the registry.
pub struct PluginRegistry {
    /// Indexed by (kind, cid).
    plugins: BTreeMap<RegistryKey, PluginMemento>,
    /// Load order: (kind, cid) pairs in the order they were registered.
    load_order: Vec<RegistryKey>,
    /// Failures minted during this run.
    failures: Vec<PluginLoadFailureMemento>,
    /// How many plugins were registered as built-ins.
    builtin_count: usize,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: BTreeMap::new(),
            load_order: Vec::new(),
            failures: Vec::new(),
            builtin_count: 0,
        }
    }

    /// Register a loaded plugin (§9.2 duplicate-CID rule).
    ///
    /// If (kind, cid) is already registered, this is a no-op (§9.2: same CID
    /// implies byte-identical content, so deduplication is safe).
    /// Returns `Ok(true)` if the plugin was newly registered, `Ok(false)` if
    /// it was already present (deduplicated).
    pub fn register(&mut self, p: PluginMemento) -> Result<bool, LoadError> {
        let key: RegistryKey = (p.kind().to_string(), p.cid().to_string());
        if self.plugins.contains_key(&key) {
            // Deduplicated — same content, no error.
            return Ok(false);
        }
        self.load_order.push(key.clone());
        self.plugins.insert(key, p);
        Ok(true)
    }

    /// Register a built-in plugin.  Built-ins are tracked separately so
    /// `emit_registry_memento` can compute `built_in_count` and append
    /// them at the end of `load_order` per §7.
    ///
    /// Must be called AFTER all user `register` calls.
    pub fn register_builtin(&mut self, p: PluginMemento) -> Result<bool, LoadError> {
        let inserted = self.register(p)?;
        if inserted {
            self.builtin_count += 1;
        }
        Ok(inserted)
    }

    /// Record a PluginLoadFailureMemento (§8).
    pub fn record_failure(&mut self, f: PluginLoadFailureMemento) {
        self.failures.push(f);
    }

    /// Lookup by (kind, cid) (§9).
    pub fn lookup(&self, kind: &str, cid: &str) -> Option<&PluginMemento> {
        self.plugins.get(&(kind.to_string(), cid.to_string()))
    }

    /// All plugins of a given kind (§9).
    pub fn by_kind(&self, kind: &str) -> Vec<&PluginMemento> {
        self.load_order
            .iter()
            .filter(|(k, _)| k == kind)
            .filter_map(|key| self.plugins.get(key))
            .collect()
    }

    /// All registered plugins in load order.
    pub fn all_in_order(&self) -> Vec<&PluginMemento> {
        self.load_order
            .iter()
            .filter_map(|key| self.plugins.get(key))
            .collect()
    }

    /// Seal the registry into a `PluginRegistryMemento` (§9.1 / §9.3).
    ///
    /// `sealed_at` should be an ISO-8601 UTC timestamp.
    /// `signer_stub` is a placeholder envelope; full signing is out-of-scope
    /// for PEP 1.7.0 skeleton (§12).
    pub fn emit_registry_memento(&self, sealed_at: &str) -> PluginRegistryMemento {
        use crate::loader::RUNTIME_PROTOCOL_VERSIONS;

        let loaded: Vec<String> = self
            .load_order
            .iter()
            .filter_map(|key| self.plugins.get(key).map(|p| p.cid().to_string()))
            .collect();

        let failure_cids: Vec<String> = self.failures.iter().map(|f| f.header.cid.clone()).collect();

        // Build load_order as CIDs.
        let load_order: Vec<String> = self
            .load_order
            .iter()
            .filter_map(|key| self.plugins.get(key).map(|p| p.cid().to_string()))
            .collect();

        let runtime_versions: Vec<String> = RUNTIME_PROTOCOL_VERSIONS
            .iter()
            .map(|s| s.to_string())
            .collect();

        // Build header without CID first (CID is computed over it).
        let mut header = PluginRegistryMementoHeader {
            built_in_count: self.builtin_count,
            cid: String::new(), // will be filled in below
            failures: failure_cids,
            kind: "plugin-registry".to_string(),
            load_order,
            loaded,
            runtime_protocol_versions: runtime_versions,
            schema_version: "1".to_string(),
            sealed_at: sealed_at.to_string(),
        };

        // Compute CID over header (§9.3) using cid.rs helper.
        header.cid = compute_registry_cid(&header);

        // Stub envelope (signing deferred per §12 skeleton scope).
        let envelope = PluginEnvelope {
            declared_at: sealed_at.to_string(),
            signature: "ed25519:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string(),
            signer: "ed25519:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".to_string(),
        };

        PluginRegistryMemento { envelope, header }
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience: mint a `PluginLoadFailureMemento` from a `LoadError`.
///
/// `declared_source` is the CLI flag value verbatim, e.g. `"sugar:./my.json"`.
/// `plugin_kind` is the declared kind from the CLI flag.
/// `failed_at` is an ISO-8601 UTC timestamp.
pub fn mint_failure_memento(
    declared_source: &str,
    plugin_kind: &str,
    error: &LoadError,
    failed_at: &str,
) -> PluginLoadFailureMemento {
    use crate::cid::compute_failure_cid;

    let mut header = PluginLoadFailureMementoHeader {
        cid: String::new(), // filled below
        declared_source: declared_source.to_string(),
        failure_at: failed_at.to_string(),
        kind: "plugin-load-failure".to_string(),
        plugin_kind: plugin_kind.to_string(),
        reason_detail: error.reason_detail(),
        reason_kind: error.reason_kind(),
        schema_version: "1".to_string(),
    };
    header.cid = compute_failure_cid(&header);

    let envelope = PluginEnvelope {
        declared_at: failed_at.to_string(),
        signature: "ed25519:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string(),
        signer: "ed25519:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".to_string(),
    };

    PluginLoadFailureMemento {
        envelope,
        header,
        metadata: Default::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{PluginEnvelope, PluginHeader, PluginMemento, PluginMetadata};

    fn dummy_memento(kind: &str, cid: &str) -> PluginMemento {
        PluginMemento {
            envelope: PluginEnvelope {
                declared_at: "2026-05-12T00:00:00.000Z".to_string(),
                signature: "ed25519:sig".to_string(),
                signer: "ed25519:pub".to_string(),
            },
            header: PluginHeader {
                cid: cid.to_string(),
                content: serde_json::json!({}),
                critical: false,
                kind: kind.to_string(),
                protocol_versions: vec!["pep/1.7.0".to_string()],
                provenance_cid: "blake3-512:prov".to_string(),
                schema_version: "1".to_string(),
                version: "0.1.0".to_string(),
            },
            metadata: PluginMetadata::default(),
        }
    }

    #[test]
    fn register_and_lookup() {
        let mut reg = PluginRegistry::new();
        let p = dummy_memento("sugar", "blake3-512:aaa");
        reg.register(p.clone()).unwrap();
        let found = reg.lookup("sugar", "blake3-512:aaa");
        assert!(found.is_some());
        assert_eq!(found.unwrap().cid(), "blake3-512:aaa");
    }

    #[test]
    fn lookup_miss_returns_none() {
        let reg = PluginRegistry::new();
        assert!(reg.lookup("sugar", "blake3-512:nope").is_none());
    }

    #[test]
    fn by_kind_returns_all_of_kind() {
        let mut reg = PluginRegistry::new();
        reg.register(dummy_memento("sugar", "blake3-512:aaa")).unwrap();
        reg.register(dummy_memento("sugar", "blake3-512:bbb")).unwrap();
        reg.register(dummy_memento("loss-function", "blake3-512:ccc")).unwrap();
        assert_eq!(reg.by_kind("sugar").len(), 2);
        assert_eq!(reg.by_kind("loss-function").len(), 1);
        assert_eq!(reg.by_kind("lifter").len(), 0);
    }

    #[test]
    fn duplicate_cid_deduplication() {
        let mut reg = PluginRegistry::new();
        let p = dummy_memento("sugar", "blake3-512:aaa");
        let r1 = reg.register(p.clone()).unwrap();
        let r2 = reg.register(p.clone()).unwrap();
        assert!(r1);  // first registration
        assert!(!r2); // deduplicated
        assert_eq!(reg.all_in_order().len(), 1);
    }

    #[test]
    fn emit_registry_memento_round_trip() {
        let mut reg = PluginRegistry::new();
        reg.register(dummy_memento("sugar", "blake3-512:aaa")).unwrap();
        let m = reg.emit_registry_memento("2026-05-12T00:00:00.000Z");
        assert!(!m.header.cid.is_empty());
        assert!(m.header.cid.starts_with("blake3-512:"));
        assert_eq!(m.header.loaded, vec!["blake3-512:aaa".to_string()]);
        assert_eq!(m.header.load_order, vec!["blake3-512:aaa".to_string()]);
        assert_eq!(m.header.built_in_count, 0);
    }

    #[test]
    fn registry_cid_is_stable_across_calls() {
        let mut reg = PluginRegistry::new();
        reg.register(dummy_memento("sugar", "blake3-512:aaa")).unwrap();
        let m1 = reg.emit_registry_memento("2026-05-12T00:00:00.000Z");
        let m2 = reg.emit_registry_memento("2026-05-12T00:00:00.000Z");
        assert_eq!(m1.header.cid, m2.header.cid);
    }

    #[test]
    fn failure_memento_minting() {
        let err = LoadError::FileNotFound {
            path: "sugar:./missing.json".to_string(),
        };
        let f = mint_failure_memento(
            "sugar:./missing.json",
            "sugar",
            &err,
            "2026-05-12T00:00:00.000Z",
        );
        assert!(f.header.cid.starts_with("blake3-512:"));
        assert_eq!(
            f.header.reason_kind,
            crate::types::FailureReasonKind::FileNotFound
        );
        assert_eq!(f.header.plugin_kind, "sugar");
    }

    #[test]
    fn builtin_count_tracks_register_builtin() {
        let mut reg = PluginRegistry::new();
        reg.register(dummy_memento("sugar", "blake3-512:user")).unwrap();
        reg.register_builtin(dummy_memento("loss-function", "blake3-512:builtin")).unwrap();
        let m = reg.emit_registry_memento("2026-05-12T00:00:00.000Z");
        assert_eq!(m.header.built_in_count, 1);
    }
}
