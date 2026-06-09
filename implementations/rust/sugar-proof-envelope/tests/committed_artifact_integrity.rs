//! Committed-artifact integrity gate.
//!
//! Every file whose name is a CID -- `blake3-512:<128hex>[.ext]`, or a bare
//! `<128hex>[.ext]` -- is a content-addressed artifact: its name IS the
//! blake3-512 of its bytes. A repo-wide text edit (a brand rename, a careless
//! sed) that rewrites such a file's content under its fixed name silently
//! breaks the trust-root invariant `load_all_proofs` enforces at load time.
//!
//! The acid-test gate never notices, because it RE-MINTS proofs rather than
//! loading the committed ones. This test loads them: it walks the repo and
//! asserts every CID-named file still hashes to its own name. It is the
//! standing regression guard for the Sugar cutover, which mangled 8 `.proof`
//! bundles (PR #1966) before PR #1970 restored them. Add nothing to bypass it.

use std::fs;
use std::path::{Path, PathBuf};

use sugar_canonicalizer::{blake3_512_of, BLAKE3_512_PREFIX};

const SKIP_DIRS: &[&str] = &[
    "target",
    ".git",
    ".jj",
    "node_modules",
    ".venv",
    "venv",
    "__pycache__",
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|p| p.join("implementations/rust").is_dir())
        .expect("repo root: an ancestor containing implementations/rust")
        .to_path_buf()
}

/// The CID a content-addressed filename claims, or `None` if the name is not a
/// CID. Accepts both the prefixed (`blake3-512:<hex>`) and bare (`<hex>`) forms;
/// the hex is the stem before the first extension dot and must be exactly 128
/// chars. This rejects e.g. `invalid-filename-cid.proof` (a deliberate negative
/// fixture) whose stem is not 128 hex.
fn claimed_cid(file_name: &str) -> Option<String> {
    let s = file_name
        .strip_prefix(BLAKE3_512_PREFIX)
        .unwrap_or(file_name);
    let stem = s.split('.').next().unwrap_or("");
    if stem.len() == 128 && stem.bytes().all(|b| b.is_ascii_hexdigit()) {
        Some(format!("{BLAKE3_512_PREFIX}{stem}"))
    } else {
        None
    }
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if !SKIP_DIRS.contains(&name.as_ref()) {
                collect(&path, out);
            }
        } else if claimed_cid(&name).is_some() {
            out.push(path);
        }
    }
}

#[test]
fn every_committed_cid_named_file_hashes_to_its_filename() {
    let root = repo_root();
    let mut files = Vec::new();
    collect(&root, &mut files);
    assert!(
        !files.is_empty(),
        "found no CID-named artifacts under {} -- the scan is broken, not the tree",
        root.display()
    );

    let mut mismatches = Vec::new();
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let claimed = claimed_cid(&name).expect("collected only CID-named files");
        let bytes = fs::read(path).expect("read content-addressed artifact");
        let actual = blake3_512_of(&bytes);
        if actual != claimed {
            mismatches.push(format!(
                "  {}\n      claims {claimed}\n      hashes {actual}",
                path.strip_prefix(&root).unwrap_or(path).display()
            ));
        }
    }

    assert!(
        mismatches.is_empty(),
        "{} content-addressed artifact(s) no longer hash to their own filename. \
         Their sealed bytes were edited under a fixed CID name (a rename or hand-edit). \
         Restore the original bytes; never rewrite a content-addressed file:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );

    eprintln!(
        "integrity: {} content-addressed artifacts verified",
        files.len()
    );
}
