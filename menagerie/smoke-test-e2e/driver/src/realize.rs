// SPDX-License-Identifier: Apache-2.0
//
// Realize: emit rewritten source files with substrate-attributed
// contracts and concept annotations.
//
// For every fixture .rs file under src/, copy the file into
// rewritten/<same-name>.rs and insert, immediately above each fn:
//
//     // concept: <name>                                    <-- naming round-trip seed
//     #[cfg_attr(any(), requires(<pre>))]                   <-- if pre present
//     #[cfg_attr(any(), ensures(<post>))]                   <-- if post present
//     #[cfg_attr(any(), witness(<pretty_formula>))]         <-- if witness inherited
//
// The `cfg_attr(any(), ...)` wrapping keeps rewritten files compiling
// under stable rustc while preserving the lifter's substrate input.
// We also copy tests/ verbatim so cargo test still runs on the
// rewritten tree.
//
// The annotations above an already-attribute-lifted function are NOT
// added if they would duplicate the source's existing annotation
// (this keeps round-trip-stability: lift -> rewrite -> lift -> rewrite
// must converge after the first pass).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::{BindingRecord, ConceptRecord, ContractOrigin, PassResult, WitnessRecord};

pub fn write_rewritten(fixture_root: &Path, rewritten_dir: &Path, pass: &PassResult) {
    // Group bindings by file so we rewrite each source once.
    let mut by_file: BTreeMap<String, Vec<&BindingRecord>> = BTreeMap::new();
    for b in &pass.bindings {
        by_file.entry(b.site_file.clone()).or_default().push(b);
    }

    // Index witnesses by every shape-CID (including aliases) of the
    // concept the witness fired on. This is how propagation reaches
    // bindings whose lifted shape is a syntactically distinct alias
    // of the named concept.
    let mut witness_by_shape: BTreeMap<String, &WitnessRecord> = BTreeMap::new();
    for w in &pass.witnesses {
        witness_by_shape.insert(w.concept_shape_cid.clone(), w);
        // Find the concept whose primary or alias shape equals
        // w.concept_shape_cid and broadcast to every alias too.
        for c in &pass.concepts {
            if c.shape_cid == w.concept_shape_cid
                || c.shape_cid_aliases.contains(&w.concept_shape_cid)
            {
                witness_by_shape.insert(c.shape_cid.clone(), w);
                for alias in &c.shape_cid_aliases {
                    witness_by_shape.insert(alias.clone(), w);
                }
            }
        }
    }

    // Also ensure tests/ are copied so cargo test on the rewritten tree
    // still finds the test-lift source.
    let tests_in = fixture_root.join("tests");
    let tests_out = rewritten_dir.join("tests");
    if tests_in.exists() {
        let _ = fs::create_dir_all(&tests_out);
        if let Ok(entries) = fs::read_dir(&tests_in) {
            for e in entries.flatten() {
                let p = e.path();
                if let Some(name) = p.file_name() {
                    let _ = fs::copy(&p, tests_out.join(name));
                }
            }
        }
    }

    // Copy Cargo.toml of the fixture (the rewritten tree is a usable
    // crate root under rewritten/).
    let cargo_in = fixture_root.join("Cargo.toml");
    if cargo_in.exists() {
        if let Ok(orig) = fs::read_to_string(&cargo_in) {
            let modified = orig
                .replace(
                    "name = \"smoke-test-e2e\"",
                    "name = \"smoke-test-e2e-rewritten\"",
                )
                .replace(
                    "name = \"smoke_test_e2e\"",
                    "name = \"smoke_test_e2e_rewritten\"",
                );
            let _ = fs::write(rewritten_dir.join("Cargo.toml"), modified);
        }
    }

    let src_out_dir = rewritten_dir.join("src");
    let _ = fs::create_dir_all(&src_out_dir);

    for (rel_file, bindings) in &by_file {
        let in_path = fixture_root.join(rel_file);
        let Ok(orig) = fs::read_to_string(&in_path) else {
            continue;
        };

        let header = format!(
            "// rewritten by smoke-test-e2e-driver pass {}\n//\n// Every contract attribute and concept annotation below was emitted\n// by the substrate. None were written by the driver author. See\n// report.md \u{00A7}8 for the per-line origin trace.\n\n",
            pass.pass_id
        );

        let mut rewritten = header.clone();
        rewritten.push_str(&rewrite_source(
            &orig,
            bindings,
            &pass.concepts,
            &witness_by_shape,
        ));

        let out_path = if rel_file.starts_with("src/") {
            src_out_dir.join(rel_file.trim_start_matches("src/"))
        } else {
            rewritten_dir.join(rel_file)
        };
        if let Some(parent) = out_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(out_path, rewritten);
    }
}

fn rewrite_source(
    orig: &str,
    bindings: &[&BindingRecord],
    concepts: &[ConceptRecord],
    witness_by_shape: &BTreeMap<String, &WitnessRecord>,
) -> String {
    // Build a fn_name -> binding lookup.
    let mut by_fn: BTreeMap<String, &BindingRecord> = BTreeMap::new();
    for b in bindings {
        by_fn.insert(b.site_fn.clone(), *b);
    }

    let mut out_lines: Vec<String> = Vec::new();
    let lines: Vec<&str> = orig.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        // Look for a fn declaration; rewrite the block of attrs above it.
        if let Some(fn_name) = parse_fn_name(line) {
            // Determine indentation of the fn line.
            let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();

            // Decide whether the immediately-preceding line already has
            // our marker block ("rewritten by smoke-test-e2e-driver"
            // separator), to avoid double-annotation when this rewriter
            // runs over its own output.
            //
            // We walk back to skip an existing attribute/comment cluster
            // (lines starting with `#[`, `///`, `//`, or empty) and
            // strip them from the output before inserting fresh ones.
            let mut start = out_lines.len();
            while start > 0 {
                let prev = out_lines[start - 1].trim_start();
                if prev.is_empty()
                    || prev.starts_with("// concept:")
                    || prev.starts_with("#[cfg_attr(any(), requires")
                    || prev.starts_with("#[cfg_attr(any(), ensures")
                    || prev.starts_with("#[cfg_attr(any(), witness")
                {
                    start -= 1;
                } else {
                    break;
                }
            }
            // Also strip a single trailing blank if present.
            out_lines.truncate(start);

            if let Some(b) = by_fn.get(&fn_name) {
                // Emit a blank line before the substrate-injected block
                // for readability when separating from previous item.
                if !out_lines.is_empty()
                    && !out_lines
                        .last()
                        .map(|s| s.trim().is_empty())
                        .unwrap_or(false)
                {
                    out_lines.push(String::new());
                }

                let concept = &concepts[b.concept_idx];
                out_lines.push(format!(
                    "{}// concept: {}",
                    indent,
                    name_for_annotation(&concept.name)
                ));

                // Origin trace as a comment so the rewritten output is
                // self-describing.
                out_lines.push(format!(
                    "{}// substrate-origin: {}",
                    indent,
                    b.contract_origin.label()
                ));
                if let Some(cid) = &b.contract_cid {
                    out_lines.push(format!("{}// memento-cid: {}", indent, cid));
                }
                if let Some(pre) = &b.pretty_pre {
                    out_lines.push(format!("{}#[cfg_attr(any(), requires({}))]", indent, pre));
                }
                if let Some(post) = &b.pretty_post {
                    out_lines.push(format!("{}#[cfg_attr(any(), ensures({}))]", indent, post));
                }
                if let Some(w) = witness_by_shape.get(&b.shape_cid) {
                    // Inherited witness obligation (propagation event).
                    out_lines.push(format!(
                        "{}#[cfg_attr(any(), witness({}))]",
                        indent,
                        w.pretty_formula.trim()
                    ));
                    out_lines.push(format!(
                        "{}// witness-inherited-from: {}",
                        indent, w.source_location
                    ));
                }
            }

            out_lines.push(line.to_string());
            i += 1;
            continue;
        }
        // Strip out pre-existing concept comments and cfg_attr lines
        // emitted by an earlier driver pass; the next pass will
        // re-emit canonicalized versions.
        let trimmed = line.trim_start();
        if trimmed.starts_with("// concept:")
            || trimmed.starts_with("// substrate-origin:")
            || trimmed.starts_with("// memento-cid:")
            || trimmed.starts_with("// witness-inherited-from:")
            || trimmed.starts_with("#[cfg_attr(any(), requires")
            || trimmed.starts_with("#[cfg_attr(any(), ensures")
            || trimmed.starts_with("#[cfg_attr(any(), witness")
        {
            // Skip — these will be re-emitted above the next fn.
            // EXCEPT: if a previous concept comment is the only
            // upstream source of the human-supplied name, we MUST
            // preserve it on disk for the next pass to read it. The
            // pass that consumes the annotation passes
            // read_concept_comments=true; here we only filter what
            // GOES INTO the rewritten output, not what we lifted.
            i += 1;
            continue;
        }
        out_lines.push(line.to_string());
        i += 1;
    }
    out_lines.join("\n") + "\n"
}

fn parse_fn_name(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let after = if let Some(s) = trimmed.strip_prefix("pub fn ") {
        s
    } else if let Some(s) = trimmed.strip_prefix("fn ") {
        s
    } else if let Some(s) = trimmed.strip_prefix("pub(crate) fn ") {
        s
    } else {
        return None;
    };
    let name: String = after
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn name_for_annotation(name: &str) -> String {
    // `concept:foo` becomes `foo`; UNNAMED-CONCEPT-N stays verbatim.
    if let Some(rest) = name.strip_prefix("concept:") {
        rest.to_string()
    } else {
        name.to_string()
    }
}

#[allow(dead_code)]
fn _kill_warning(_: ContractOrigin) {}
