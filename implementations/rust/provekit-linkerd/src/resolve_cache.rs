// SPDX-License-Identifier: Apache-2.0
//
// resolve_cache.rs: content-addressed per-file callee-resolution cache.
//
// Specs #1705 (content-addressed callee-resolution cache) and #1706 (per-file
// resolution cache with dependency-set granularity), serving #1707.
//
// THE INSIGHT (ProvekIt-native, "if you can't content-address it, it doesn't
// belong"): resolving a method-call position in a file to a crate is
// DETERMINISTIC given the file's content plus the dependency set. So it is
// content-addressable. A warm rust-analyzer alone still pays the ~260s index on
// every COLD daemon start; a content-addressed cache persisted to disk skips
// rust-analyzer ENTIRELY on unchanged inputs, even from a fresh daemon process.
//
// KEY (#1705): `(blake3(file_content), depSetCid)`. The workspace root scopes
// the on-disk file. The file-content hash makes editing one file invalidate
// ONLY that file's entries (#1706 per-file granularity), never the workspace.
// The dep-set CID (blake3 of Cargo.lock for the MVP) invalidates every file when
// the dependency graph changes, because a re-export or version bump can move a
// callee to a different crate.
//
// VALUE: the COMPLETE resolution map for that file from one QUIESCENT pass:
// `{ "<line>:<col>": "<crate>" }`. A position that was resolved appears with its
// crate; a position that the oracle DETERMINISTICALLY refused (null definition,
// unmappable path, ambiguous) is recorded as a refusal so a cache hit reproduces
// the refusal WITHOUT re-asking RA. A file is only cached when EVERY queried
// position settled (resolved or deterministic-refuse) from a quiescent session;
// a not-ready/churn pass is never cached (a partial entry would wrongly suppress
// RA on a later run). This is what keeps the refuse-floor intact across caching.

use std::collections::BTreeMap;
use std::path::Path;

use provekit_canonicalizer::blake3_512_of;
use serde::{Deserialize, Serialize};

/// One position's resolution outcome inside a file's cache entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PosOutcome {
    /// Resolved to a defining crate.
    Crate(String),
    /// Deterministically refused (null definition / unmappable / ambiguous).
    /// Recorded so a cache hit reproduces the refusal with no RA spawn.
    Refused,
}

/// The cached resolution for one file at one (content, dep-set) state.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileResolution {
    /// "<line>:<col>" -> outcome. Complete for the positions queried in the
    /// quiescent pass that produced it.
    pub positions: BTreeMap<String, PosOutcome>,
}

/// The persisted cache: content-address key -> file resolution.
///
/// The key is `"<file_content_blake3>|<dep_set_cid>"`. Two different files with
/// identical content AND dep-set legitimately share an entry: resolution depends
/// only on content + deps, not on the path. This is the content-addressing
/// invariant, not a bug.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResolveCache {
    entries: BTreeMap<String, FileResolution>,
}

impl ResolveCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Compute the content-address key for a file's bytes under a dep-set CID.
    pub fn key(file_content: &[u8], dep_set_cid: &str) -> String {
        let content_cid = blake3_512_of(file_content);
        format!("{content_cid}|{dep_set_cid}")
    }

    /// Look up a file's complete resolution by content + dep-set. A hit is
    /// AUTHORITATIVE for the whole file: every position it carries is final
    /// (resolved or refused), and the file contributes ZERO queries to RA.
    pub fn get(&self, file_content: &[u8], dep_set_cid: &str) -> Option<&FileResolution> {
        self.entries.get(&Self::key(file_content, dep_set_cid))
    }

    /// Store a file's complete resolution. Only call this with the outcome of a
    /// QUIESCENT pass in which every queried position settled; never with a
    /// partial/not-ready result.
    pub fn insert(&mut self, file_content: &[u8], dep_set_cid: &str, resolution: FileResolution) {
        self.entries
            .insert(Self::key(file_content, dep_set_cid), resolution);
    }

    /// Serialise to bytes for the sidecar.
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// Restore from sidecar bytes; an unreadable cache starts empty (it is a
    /// cache, never a source of truth: a miss just re-asks RA).
    pub fn from_bytes(bytes: &[u8]) -> Self {
        serde_json::from_slice(bytes).unwrap_or_default()
    }
}

/// Compute the dependency-set CID for a workspace.
///
/// MVP simplification (documented): blake3 of `Cargo.lock`. The lock file pins
/// every resolved dependency version, so it invalidates correctly when a dep is
/// added/removed/bumped, which is exactly when a callee could move to a
/// different crate. Falls back to a fixed sentinel when no lock file exists
/// (a workspace with no resolved deps): resolution then depends on content
/// alone, which is sound for a std-only crate. A richer key (the cargo metadata
/// graph CID) is a future refinement; the cache stays correct either way
/// because a wrong key only causes a MISS (re-ask RA), never a wrong hit.
pub fn dep_set_cid(workspace_root: &Path) -> String {
    // Search the workspace root and its ancestors for a Cargo.lock (cargo
    // workspaces keep one lock at the workspace root above member crates).
    let mut dir = Some(workspace_root);
    while let Some(d) = dir {
        let lock = d.join("Cargo.lock");
        if let Ok(bytes) = std::fs::read(&lock) {
            return blake3_512_of(&bytes);
        }
        dir = d.parent();
    }
    "no-cargo-lock".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_changes_with_content() {
        let k1 = ResolveCache::key(b"fn a() {}", "deps");
        let k2 = ResolveCache::key(b"fn b() {}", "deps");
        assert_ne!(k1, k2);
    }

    #[test]
    fn key_changes_with_dep_set() {
        let k1 = ResolveCache::key(b"same", "deps-1");
        let k2 = ResolveCache::key(b"same", "deps-2");
        assert_ne!(k1, k2);
    }

    #[test]
    fn hit_is_authoritative_for_whole_file() {
        let mut cache = ResolveCache::new();
        let mut res = FileResolution::default();
        res.positions
            .insert("3:7".into(), PosOutcome::Crate("std".into()));
        res.positions.insert("4:1".into(), PosOutcome::Refused);
        cache.insert(b"content", "deps", res);

        let hit = cache.get(b"content", "deps").unwrap();
        assert_eq!(
            hit.positions.get("3:7"),
            Some(&PosOutcome::Crate("std".into()))
        );
        assert_eq!(hit.positions.get("4:1"), Some(&PosOutcome::Refused));
        // A different content is a miss.
        assert!(cache.get(b"changed", "deps").is_none());
    }

    #[test]
    fn roundtrips_through_bytes() {
        let mut cache = ResolveCache::new();
        let mut res = FileResolution::default();
        res.positions
            .insert("1:0".into(), PosOutcome::Crate("serde_json".into()));
        cache.insert(b"x", "d", res);
        let bytes = cache.to_bytes();
        let restored = ResolveCache::from_bytes(&bytes);
        assert_eq!(restored.len(), 1);
        assert_eq!(
            restored
                .get(b"x", "d")
                .unwrap()
                .positions
                .get("1:0"),
            Some(&PosOutcome::Crate("serde_json".into()))
        );
    }
}
