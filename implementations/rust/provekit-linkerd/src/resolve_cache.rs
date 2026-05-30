// SPDX-License-Identifier: Apache-2.0
//
// resolve_cache.rs: content-addressed per-file callee-resolution cache.
//
// Specs #1705 (content-addressed callee-resolution cache) and #1706 (per-file
// resolution cache with dependency-set granularity), serving #1707.
//
// THE INSIGHT (ProvekIt-native, "if you can't content-address it, it doesn't
// belong"): resolving a method-call position to a crate is DETERMINISTIC given
// the resolver's full INPUT, so it is content-addressable. A warm rust-analyzer
// alone still pays the workspace index on every COLD daemon start; a
// content-addressed cache persisted to disk skips rust-analyzer ENTIRELY when
// that input is unchanged, even from a fresh daemon process.
//
// KEY (#1705): `(blake3(file_content), resolutionContextCid)`.
//
// HONESTY ABOUT THE INPUT (this is the subtle part, supra omnia rectum). A
// position's resolution is NOT a pure function of (this file's bytes + the
// dependency lock). Rust type inference flows ACROSS files: `let c =
// open_conn(); c.query()` resolves `query` against the type `open_conn` returns,
// which is declared in ANOTHER file. If that other file changes (e.g. the
// connection switches sqlite -> postgres, both registry crates), THIS file's
// bytes and Cargo.lock are unchanged, yet the correct resolution changed. Keying
// on this file alone would then serve a STALE hit and emit a wrong bridge: the
// exact cross-library confusion ProvekIt exists to prevent. So a per-file key is
// UNSOUND on its own.
//
// We therefore fold the WHOLE in-workspace source tree into
// `resolutionContextCid` (blake3 of Cargo.lock plus the sorted hashes of every
// workspace `.rs` file). Any in-workspace edit changes the context CID and
// invalidates every file's entry. This is deliberately CONSERVATIVE: it
// over-invalidates (an edit to an unrelated file also drops the cache) in
// exchange for soundness (a hit can never reflect stale cross-file type flow).
// A wrong key only ever causes a MISS (re-ask the warm RA), never a wrong HIT.
// A precise dependency-set-per-file key (#1706's finest granularity) is a future
// refinement; the conservative tree hash is the sound MVP.
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
    /// Resolved to a defining crate, with the best-effort receiver-type stem
    /// (None when the crate was definite but the type could not be
    /// disambiguated). The stem is cached alongside the crate so a cache hit
    /// reproduces the disambiguated panic-partial selection with no RA spawn.
    Crate {
        krate: String,
        type_stem: Option<String>,
    },
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

/// Compute the resolution-context CID: the SECOND key component, capturing every
/// input a position's resolution depends on BEYOND its own file's bytes.
///
/// It folds two things, in this order:
///   1. The dependency lock (`Cargo.lock`): a re-export or version bump can move
///      a callee to a different crate, so a dep change must invalidate.
///   2. A hash of the WHOLE in-workspace `.rs` source tree: rust type inference
///      flows across files (a receiver's type can be declared in another file),
///      so an edit to ANY workspace file can change THIS file's correct
///      resolution. Folding the tree makes any in-workspace edit invalidate
///      every entry. This is deliberately CONSERVATIVE (over-invalidates) to be
///      SOUND: a hit can never reflect stale cross-file type flow. A wrong key
///      only ever causes a MISS (re-ask the warm RA), never a wrong HIT.
///
/// Files are visited in sorted path order so the fold is deterministic. The walk
/// is bounded to `.rs` files and skips `target/` and dotted dirs (build output /
/// VCS are not resolver inputs). A missing Cargo.lock contributes a sentinel:
/// resolution then depends on the source tree alone, sound for a std-only crate.
pub fn resolution_context_cid(workspace_root: &Path) -> String {
    use walkdir::WalkDir;

    // 1. Dependency lock (search root + ancestors: a cargo workspace keeps one
    //    lock at the workspace root above member crates).
    let mut lock_bytes: Vec<u8> = b"no-cargo-lock".to_vec();
    let mut dir = Some(workspace_root);
    while let Some(d) = dir {
        let lock = d.join("Cargo.lock");
        if let Ok(bytes) = std::fs::read(&lock) {
            lock_bytes = bytes;
            break;
        }
        dir = d.parent();
    }

    // 2. Sorted per-file content hashes of the workspace `.rs` tree.
    let mut file_hashes: Vec<(String, String)> = Vec::new();
    for entry in WalkDir::new(workspace_root)
        .into_iter()
        .filter_entry(|e| {
            // Skip target/ and dotted dirs (build output, VCS): not resolver
            // inputs, and target/ can be huge.
            let name = e.file_name().to_string_lossy();
            !(e.file_type().is_dir() && (name == "target" || name.starts_with('.')))
        })
        .filter_map(Result::ok)
    {
        if entry.file_type().is_file()
            && entry.path().extension().and_then(|s| s.to_str()) == Some("rs")
        {
            if let Ok(bytes) = std::fs::read(entry.path()) {
                file_hashes.push((
                    entry.path().to_string_lossy().into_owned(),
                    blake3_512_of(&bytes),
                ));
            }
        }
    }
    file_hashes.sort();

    // Fold lock + sorted (path, content-hash) pairs into one CID.
    let mut acc = String::new();
    acc.push_str(&blake3_512_of(&lock_bytes));
    for (path, hash) in &file_hashes {
        acc.push('\n');
        acc.push_str(path);
        acc.push('\0');
        acc.push_str(hash);
    }
    blake3_512_of(acc.as_bytes())
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
        res.positions.insert(
            "3:7".into(),
            PosOutcome::Crate {
                krate: "std".into(),
                type_stem: Some("option".into()),
            },
        );
        res.positions.insert("4:1".into(), PosOutcome::Refused);
        cache.insert(b"content", "deps", res);

        let hit = cache.get(b"content", "deps").unwrap();
        assert_eq!(
            hit.positions.get("3:7"),
            Some(&PosOutcome::Crate {
                krate: "std".into(),
                type_stem: Some("option".into()),
            })
        );
        assert_eq!(hit.positions.get("4:1"), Some(&PosOutcome::Refused));
        // A different content is a miss.
        assert!(cache.get(b"changed", "deps").is_none());
    }

    #[test]
    fn roundtrips_through_bytes() {
        let mut cache = ResolveCache::new();
        let mut res = FileResolution::default();
        res.positions.insert(
            "1:0".into(),
            PosOutcome::Crate {
                krate: "serde_json".into(),
                type_stem: None,
            },
        );
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
            Some(&PosOutcome::Crate {
                krate: "serde_json".into(),
                type_stem: None,
            })
        );
    }
}
