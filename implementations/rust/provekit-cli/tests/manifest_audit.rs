// #1364 audit-as-test (foundation chunk): verifies every realize manifest's
// declared provides_concepts is internally consistent with its source of
// truth — either the matching body-templates JSON (canonical per-kit
// concept inventory) OR, for rust-shim-* manifests, the shim's signed
// .proof envelope library-sugar-binding-entry members.
//
// This catches manifest/.proof drift before it lands: if a kit changes
// concept coverage and the manifest forgets to update (or vice versa),
// the audit fires.
//
// Scope: rust manifests against body-templates JSONs / rust shim .proof
// envelopes. TS / Python / Java manifests are also covered through their
// respective body-templates JSONs.
//
// Out of scope: substrate-IR-primitive concepts (concept:closure,
// concept:reference, etc.) which the realize binary lowers structurally
// from term_shape, not via body-templates. These never appear in
// provides_concepts.

use std::fs;
use std::path::{Path, PathBuf};

use provekit_proof_envelope::cbor_decode;
use serde_json::Value;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
}

/// Parse a realize manifest TOML file and return its declared
/// provides_concepts list, library_tag, and optional `sugar_proof` pointer.
/// Uses the same line-based parsing kit_dispatch.rs::parse_manifest uses
/// (single-line array form).
///
/// `sugar_proof`, when present, names the @ProveKitSugar shim project (a dir
/// we glob for `*.proof`, or a specific `.proof`) whose signed
/// `library-sugar-binding-entry` members are the AUTHORITY for this kit's
/// emission bodies. It supersedes the on-disk body-templates JSON as the
/// source-of-truth (the JSON is then deletable), mirroring the realize
/// path's `.proof`-load-via-RPC enabler (#1460) and the migrate path's
/// `body_template_cid` enabler.
fn read_provides_concepts(manifest_path: &Path) -> (String, Vec<String>, Option<String>) {
    let raw = fs::read_to_string(manifest_path)
        .unwrap_or_else(|_| panic!("read manifest {manifest_path:?}"));
    let mut library_tag = String::new();
    let mut concepts: Vec<String> = Vec::new();
    let mut sugar_proof: Option<String> = None;
    for line in raw.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("library_tag") {
            let val = rest.trim_start().trim_start_matches('=').trim();
            library_tag = val.trim_matches('"').to_string();
        }
        if let Some(rest) = trimmed.strip_prefix("provides_concepts") {
            let val = rest.trim_start().trim_start_matches('=').trim();
            concepts = parse_toml_string_array(val);
        }
        if let Some(rest) = trimmed.strip_prefix("sugar_proof") {
            let val = rest.trim_start().trim_start_matches('=').trim();
            let pointer = val.trim_matches('"').to_string();
            if !pointer.is_empty() {
                sugar_proof = Some(pointer);
            }
        }
    }
    (library_tag, concepts, sugar_proof)
}

/// Mirror of kit_dispatch::parse_toml_string_array (quote-aware splitter
/// for single-line TOML string arrays).
fn parse_toml_string_array(raw: &str) -> Vec<String> {
    let stripped = raw.trim().trim_start_matches('[').trim_end_matches(']');
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    for ch in stripped.chars() {
        if ch == '"' {
            in_quote = !in_quote;
            current.push(ch);
        } else if ch == ',' && !in_quote {
            parts.push(current.trim().trim_matches('"').to_string());
            current.clear();
        } else {
            current.push(ch);
        }
    }
    if !current.trim().is_empty() {
        parts.push(current.trim().trim_matches('"').to_string());
    }
    parts.into_iter().filter(|s| !s.is_empty()).collect()
}

/// Read the body-templates JSON for `(target_lang, library_tag)` and
/// return the set of concept_name values it declares.
fn body_template_concepts(
    root: &Path,
    target_lang: &str,
    library_tag: &str,
) -> Option<Vec<String>> {
    let candidates = [
        // Standard form.
        root.join("menagerie")
            .join(format!("{target_lang}-language-signature"))
            .join("specs")
            .join("body-templates")
            .join(format!("{target_lang}-canonical-bodies-{library_tag}.json")),
        // Catch-all (omit library_tag in filename).
        root.join("menagerie")
            .join(format!("{target_lang}-language-signature"))
            .join("specs")
            .join("body-templates")
            .join(format!("{target_lang}-canonical-bodies.json")),
    ];
    for path in &candidates {
        if !path.exists() {
            continue;
        }
        let raw = fs::read_to_string(path).ok()?;
        let doc: Value = serde_json::from_str(&raw).ok()?;
        let entries = doc
            .get("header")?
            .get("content")?
            .get("entries")?
            .as_array()?;
        let mut concepts: Vec<String> = entries
            .iter()
            .filter_map(|e| e.get("concept_name")?.as_str().map(String::from))
            .collect();
        concepts.sort();
        concepts.dedup();
        return Some(concepts);
    }
    None
}

/// Read a rust-shim-* manifest's source-of-truth: the shim's .proof
/// envelope at the matching examples/<shim>/blake3-512:*.proof.
/// Returns the set of concept_name values from library-sugar-binding-entry
/// members.
fn shim_proof_concepts(root: &Path, library_tag: &str) -> Option<Vec<String>> {
    let shim_dir = root.join("examples").join(library_tag);
    if !shim_dir.is_dir() {
        return None;
    }
    let proof_path = fs::read_dir(&shim_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .find(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.starts_with("blake3-512:") && name.ends_with(".proof")
        })?
        .path();
    let raw = fs::read(&proof_path).ok()?;
    let doc: Value = serde_json::from_slice(&raw).ok()?;
    let members = doc.get("members")?.as_object()?;
    let mut concepts: Vec<String> = members
        .values()
        .filter_map(|m| {
            let body = m.get("body")?;
            if body.get("kind")?.as_str() != Some("library-sugar-binding-entry") {
                return None;
            }
            body.get("concept_name")?.as_str().map(String::from)
        })
        .collect();
    concepts.sort();
    concepts.dedup();
    Some(concepts)
}

/// Read the concept_name set from a shim `.proof` source-of-truth named by a
/// manifest's `sugar_proof` pointer (resolved against `root`).
///
/// This is the source-of-truth selector for ANY realize kit that declares a
/// `sugar_proof` (java-json/gson, python-sqlite3, etc.) — the @ProveKitSugar
/// shim `.proof` is authoritative once the on-disk body-templates JSON is
/// deleted. Discovery mirrors `cmd_materialize::body_templates_from_shim_proof`:
/// a directory globs its `*.proof`; a file is used directly. Only
/// `library-sugar-binding-entry` members whose `target_library_tag` matches
/// `library_tag` count, so a multi-tag `.proof` audits the right slice.
/// Returns the deduped, sorted concept_name set, or `None` if nothing resolves
/// (caller treats that as un-auditable, not a failure).
fn shim_proof_concepts_at(root: &Path, pointer: &Path, library_tag: &str) -> Option<Vec<String>> {
    let pointer = if pointer.is_absolute() {
        pointer.to_path_buf()
    } else {
        root.join(pointer)
    };

    let mut proof_files: Vec<PathBuf> = Vec::new();
    if pointer.is_dir() {
        for entry in fs::read_dir(&pointer).ok()?.flatten() {
            let path = entry.path();
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".proof"))
            {
                proof_files.push(path);
            }
        }
    } else if pointer.is_file() {
        proof_files.push(pointer);
    }
    if proof_files.is_empty() {
        return None;
    }
    proof_files.sort();

    // Decode the CBOR proof envelope exactly as the realize/migrate enabler
    // does (`cbor_decode::decode` -> `members` map -> per-member bstr -> utf8 ->
    // serde_json body). NOTE: shim `.proof` envelopes are CBOR, not JSON, so a
    // `serde_json::from_slice` on the whole file would fail.
    let mut concepts: Vec<String> = Vec::new();
    for proof in &proof_files {
        let Ok(bytes) = fs::read(proof) else {
            continue;
        };
        let Ok(catalog) = cbor_decode::decode(&bytes) else {
            continue;
        };
        let Some(root) = catalog.as_map() else {
            continue;
        };
        let Some(members) = root.get("members").and_then(cbor_decode::CborValue::as_map) else {
            continue;
        };
        for member in members.values() {
            let Some(member_bytes) = member.as_bstr() else {
                continue;
            };
            let Ok(member_text) = std::str::from_utf8(member_bytes) else {
                continue;
            };
            let Ok(member_json) = serde_json::from_str::<Value>(member_text) else {
                continue;
            };
            let Some(body) = member_json.get("body") else {
                continue;
            };
            if body.get("kind").and_then(Value::as_str) != Some("library-sugar-binding-entry") {
                continue;
            }
            // Match the tag (a single-tag shim carries only its own, but the
            // guard keeps multi-tag envelopes honest — same predicate
            // `body_templates_from_shim_proof` uses).
            if body.get("target_library_tag").and_then(Value::as_str) != Some(library_tag) {
                continue;
            }
            if let Some(name) = body.get("concept_name").and_then(Value::as_str) {
                concepts.push(name.to_string());
            }
        }
    }
    if concepts.is_empty() {
        return None;
    }
    concepts.sort();
    concepts.dedup();
    Some(concepts)
}

/// Walk .provekit/realize/*/manifest.toml, run the audit per manifest,
/// return mismatches found.
fn audit_all_manifests(root: &Path) -> Vec<String> {
    let realize_dir = root.join(".provekit").join("realize");
    let mut mismatches: Vec<String> = Vec::new();

    for entry in fs::read_dir(&realize_dir).expect("read .provekit/realize/") {
        let entry = entry.expect("read realize dir entry");
        if !entry.path().is_dir() {
            continue;
        }
        let manifest_path = entry.path().join("manifest.toml");
        if !manifest_path.exists() {
            continue;
        }
        let dirname = entry.file_name().to_string_lossy().into_owned();
        let (library_tag, declared_concepts, sugar_proof) =
            read_provides_concepts(&manifest_path);

        // No provides_concepts declaration ↔ no enforcement (back-compat).
        if declared_concepts.is_empty() {
            continue;
        }

        // Target language: dir name prefix. Maps:
        //   rust* → rust, typescript* → typescript, python* → python,
        //   java* → java, c → c.
        let target_lang = if dirname.starts_with("rust") {
            "rust"
        } else if dirname.starts_with("typescript") {
            "typescript"
        } else if dirname.starts_with("python") {
            "python"
        } else if dirname.starts_with("java") {
            "java"
        } else if dirname == "c" {
            "c"
        } else {
            continue;
        };

        // Pick source of truth, in precedence order:
        //   1. `sugar_proof` declared → the named @ProveKitSugar shim `.proof`
        //      is authoritative (java-json/gson, python-sqlite3, ...). Once a
        //      kit declares a `sugar_proof`, its on-disk body-templates JSON is
        //      deletable; the manifest field — not a dir-name convention — is
        //      the actual source-of-truth signal. Falls back to the JSON only
        //      if the pointer resolves no matching entries.
        //   2. rust-shim-* (legacy convention, no `sugar_proof` field) → the
        //      shim's `examples/<tag>/blake3-512:*.proof`.
        //   3. everything else → body-templates JSON.
        let source_of_truth: Option<Vec<String>> = if let Some(pointer) = &sugar_proof {
            shim_proof_concepts_at(root, Path::new(pointer), &library_tag)
                .or_else(|| body_template_concepts(root, target_lang, &library_tag))
        } else if dirname.starts_with("rust-shim-") {
            shim_proof_concepts(root, &library_tag)
        } else {
            body_template_concepts(root, target_lang, &library_tag)
        };

        let Some(truth) = source_of_truth else {
            // No source-of-truth file found; can't audit. Note but
            // don't fail — some kits may legitimately have manifests
            // without body-templates yet (mid-flight work).
            continue;
        };

        // Audit: every declared concept must appear in source-of-truth.
        for declared in &declared_concepts {
            if !truth.contains(declared) {
                mismatches.push(format!(
                    "manifest {dirname}: declares concept `{declared}` but \
                     source-of-truth (.proof/body-templates for {library_tag}) \
                     does not contain it. Either remove from manifest or add to source."
                ));
            }
        }
    }

    mismatches
}

#[test]
fn realize_manifests_consistent_with_source_of_truth() {
    let root = repo_root();
    let mismatches = audit_all_manifests(&root);
    if !mismatches.is_empty() {
        for m in &mismatches {
            eprintln!("DRIFT: {m}");
        }
        panic!(
            "Realize manifest provides_concepts drift detected ({} mismatch(es)). \
             Either update the manifest's provides_concepts to match the \
             shim's .proof envelope / body-templates JSON, OR update the \
             source of truth to include the declared concepts.",
            mismatches.len()
        );
    }
}
