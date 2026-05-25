// #1364 audit-as-test (foundation chunk): verifies every realize manifest's
// declared provides_concepts is internally consistent with the kit-reported
// body-template entries, falling back to matching legacy body-templates JSON
// only for surfaces that have not moved to kit-owned shim resolution.
//
// This catches manifest/template drift before it lands: if a kit changes
// concept coverage and the manifest forgets to update (or vice versa),
// the audit fires.
//
// Scope: realize manifests against kit RPC or legacy body-templates JSON.
//
// Out of scope: substrate-IR-primitive concepts (concept:closure,
// concept:reference, etc.) which the realize binary lowers structurally
// from term_shape, not via body-templates. These never appear in
// provides_concepts.

use std::fs;
use std::path::{Path, PathBuf};

use provekit_cli::kit_dispatch::body_template_entries_via_rpc;
use serde_json::Value;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
}

/// Parse a realize manifest TOML file and return its declared
/// provides_concepts list and library_tag.
/// Uses the same line-based parsing kit_dispatch.rs::parse_manifest uses
/// (single-line array form).
fn read_provides_concepts(manifest_path: &Path) -> (String, Vec<String>) {
    let raw = fs::read_to_string(manifest_path)
        .unwrap_or_else(|_| panic!("read manifest {manifest_path:?}"));
    let mut library_tag = String::new();
    let mut concepts: Vec<String> = Vec::new();
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
    }
    (library_tag, concepts)
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

fn kit_rpc_concepts(root: &Path, target_lang: &str, library_tag: &str) -> Option<Vec<String>> {
    let entries = body_template_entries_via_rpc(root, target_lang, library_tag).ok()?;
    if entries.is_empty() {
        return None;
    }
    let mut concepts: Vec<String> = entries
        .iter()
        .filter_map(|entry| entry.get("concept_name")?.as_str().map(String::from))
        .collect();
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
        let (library_tag, declared_concepts) = read_provides_concepts(&manifest_path);

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

        let source_of_truth = kit_rpc_concepts(root, target_lang, &library_tag)
            .or_else(|| body_template_concepts(root, target_lang, &library_tag));

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
                     source-of-truth (kit RPC/body-templates for {library_tag}) \
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
             kit-reported entries / body-templates JSON, OR update the \
             source of truth to include the declared concepts.",
            mismatches.len()
        );
    }
}
