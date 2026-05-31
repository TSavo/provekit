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
// `resolutionContextCid` (blake3 of Cargo.lock, rust-toolchain[.toml], plus the
// sorted hashes of every workspace `.rs` file). Any in-workspace edit or
// toolchain change changes the context CID and invalidates every file's entry.
// This is deliberately CONSERVATIVE: it over-invalidates (an edit to an
// unrelated file also drops the cache) in exchange for soundness (a hit can
// never reflect stale cross-file type flow). A wrong key only ever causes a MISS
// (re-ask the warm RA), never a wrong HIT. A precise dependency-set-per-file key
// (#1706's finest granularity) is a future refinement; the conservative tree
// hash is the sound MVP.
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
/// It folds three things, in this order:
///   1. The dependency lock (`Cargo.lock`): a re-export or version bump can move
///      a callee to a different crate, so a dep change must invalidate.
///   2. The Rust toolchain pin (`rust-toolchain` or `rust-toolchain.toml`):
///      sysroot and language semantics can change with the selected toolchain.
///   3. A hash of the WHOLE in-workspace `.rs` source tree: rust type inference
///      flows across files (a receiver's type can be declared in another file),
///      so an edit to ANY workspace file can change THIS file's correct
///      resolution. Folding the tree makes any in-workspace edit invalidate
///      every entry. This is deliberately CONSERVATIVE (over-invalidates) to be
///      SOUND: a hit can never reflect stale cross-file type flow. A wrong key
///      only ever causes a MISS (re-ask the warm RA), never a wrong HIT.
///
/// Files are visited in sorted path order so the fold is deterministic. The walk
/// is bounded to `.rs` files and skips `target/` and dotted dirs (build output /
/// VCS are not resolver inputs). Paths are folded relative to `workspace_root`;
/// identical bytes in another worktree must produce the same context CID. Missing
/// Cargo/toolchain files contribute sentinels.
pub fn resolution_context_cid(workspace_root: &Path) -> String {
    use walkdir::WalkDir;

    // Search root + ancestors: a cargo workspace keeps these files at the
    // workspace root above member crates.
    let lock_bytes = read_nearest_ancestor_file(workspace_root, "Cargo.lock")
        .unwrap_or_else(|| b"no-cargo-lock".to_vec());
    let toolchain_bytes = read_nearest_ancestor_file(workspace_root, "rust-toolchain.toml")
        .or_else(|| read_nearest_ancestor_file(workspace_root, "rust-toolchain"))
        .unwrap_or_else(|| b"no-rust-toolchain".to_vec());

    // Sorted per-file content hashes of the workspace `.rs` tree.
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
                    relative_path_key(workspace_root, entry.path()),
                    blake3_512_of(&bytes),
                ));
            }
        }
    }
    file_hashes.sort();

    // Fold lock + sorted (path, content-hash) pairs into one CID.
    let mut acc = String::new();
    acc.push_str("cargo-lock\0");
    acc.push_str(&blake3_512_of(&lock_bytes));
    acc.push('\n');
    acc.push_str("rust-toolchain\0");
    acc.push_str(&blake3_512_of(&toolchain_bytes));
    for (path, hash) in &file_hashes {
        acc.push('\n');
        acc.push_str(path);
        acc.push('\0');
        acc.push_str(hash);
    }
    blake3_512_of(acc.as_bytes())
}

fn read_nearest_ancestor_file(start: &Path, name: &str) -> Option<Vec<u8>> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        let path = d.join(name);
        if let Ok(bytes) = std::fs::read(&path) {
            return Some(bytes);
        }
        dir = d.parent();
    }
    None
}

fn relative_path_key(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("provekit-{label}-{nanos}-{}", std::process::id()))
    }

    fn write_minimal_workspace(root: &Path) {
        fs::create_dir_all(root.join("src")).expect("mkdir src");
        fs::write(root.join("Cargo.lock"), "version = 4\n").expect("write Cargo.lock");
        fs::write(
            root.join("rust-toolchain.toml"),
            "[toolchain]\nchannel = \"stable\"\n",
        )
        .expect("write rust-toolchain.toml");
        fs::write(
            root.join("src").join("lib.rs"),
            "pub fn answer() -> u32 { 42 }\n",
        )
        .expect("write lib.rs");
    }

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
            restored.get(b"x", "d").unwrap().positions.get("1:0"),
            Some(&PosOutcome::Crate {
                krate: "serde_json".into(),
                type_stem: None,
            })
        );
    }

    #[test]
    fn resolution_context_is_content_addressed_not_worktree_path_addressed() {
        let root_a = unique_temp_dir("ra-cache-a");
        let root_b = unique_temp_dir("ra-cache-b");
        write_minimal_workspace(&root_a);
        write_minimal_workspace(&root_b);

        let cid_a = resolution_context_cid(&root_a);
        let cid_b = resolution_context_cid(&root_b);

        let _ = fs::remove_dir_all(&root_a);
        let _ = fs::remove_dir_all(&root_b);

        assert_eq!(
            cid_a, cid_b,
            "identical workspace bytes must produce the same resolution context CID across worktrees"
        );
    }

    #[test]
    fn resolution_context_changes_when_toolchain_changes() {
        let root = unique_temp_dir("ra-cache-toolchain");
        write_minimal_workspace(&root);

        let before = resolution_context_cid(&root);
        fs::write(
            root.join("rust-toolchain.toml"),
            "[toolchain]\nchannel = \"nightly\"\n",
        )
        .expect("rewrite rust-toolchain.toml");
        let after = resolution_context_cid(&root);

        let _ = fs::remove_dir_all(&root);

        assert_ne!(
            before, after,
            "rust-toolchain changes can affect resolution and must invalidate the cache generation"
        );
    }
}
