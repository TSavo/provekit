// SPDX-License-Identifier: Apache-2.0
//
// Naming round-trip.
//
// Sir's "money shot" demonstration. After pass 1 writes
// rewritten/src/*.rs with `// concept: UNNAMED-CONCEPT-N` above each
// unnamed cluster, a human (here: this function, acting as the
// substrate's stand-in for a human edit) replaces the first such
// annotation with a real name. The next pass reads the human-supplied
// name via attrs::extract_concept_annotation and binds the same
// shape-CID to the new name.
//
// Returns the (shape_cid, new_name) pair so the report can show the
// before-after pair explicitly.

use std::fs;
use std::path::{Path, PathBuf};

use crate::PassResult;

pub fn apply_human_naming(
    rewritten_dir: &Path,
    pass: &PassResult,
    new_name: &str,
) -> Option<(String, String)> {
    // Pick the UNNAMED concept with the MOST sites. This is the most
    // valuable rename target: a singleton-unnamed cluster is usually
    // a private helper; a multi-site unnamed cluster is a missing
    // catalog entry the human can name once and have applied
    // everywhere the shape recurs.
    let unnamed_idx = pass
        .concepts
        .iter()
        .enumerate()
        .filter(|(_, c)| c.name.starts_with("UNNAMED-CONCEPT-"))
        .max_by_key(|(_, c)| c.site_indices.len())
        .map(|(i, _)| i)?;

    let target_concept = &pass.concepts[unnamed_idx];
    let unnamed_name = target_concept.name.clone();
    let shape_cid = target_concept.shape_cid.clone();

    // Find the file that contains the first site for that concept.
    let site_idx = *target_concept.site_indices.first()?;
    let site_file = pass.bindings[site_idx].site_file.clone();
    // pass.bindings's site_file is relative to the source root used by
    // the pass; pass 1 sources are the fixture root, so rewritten path
    // is rewritten/src/<basename>.
    let basename = site_file
        .strip_prefix("src/")
        .unwrap_or(site_file.as_str());
    let target_path: PathBuf = rewritten_dir.join("src").join(basename);

    // Read, replace `// concept: <UNNAMED-CONCEPT-N>` with the human name.
    let Ok(src) = fs::read_to_string(&target_path) else {
        eprintln!(
            "[naming] rewritten file missing for pass-2 read: {}",
            target_path.display()
        );
        return None;
    };
    let pattern = format!("// concept: {}", unnamed_name);
    let replacement = format!("// concept: {}", new_name);
    if !src.contains(&pattern) {
        eprintln!(
            "[naming] expected pattern not in rewritten source: '{}'",
            pattern
        );
        return None;
    }
    let new_src = src.replace(&pattern, &replacement);
    if fs::write(&target_path, new_src).is_err() {
        return None;
    }
    Some((shape_cid, new_name.to_string()))
}
