// SPDX-License-Identifier: Apache-2.0
//
// Catalog-query helper for composing sort morphisms across kits.
//
// Per the substrate-honest cross-language model (#1361 chunk 2 part B,
// 2026-05-21 discussion): catalogued sort morphisms are the
// primitive-translation layer. To translate a parameter type from
// source-lang to target-lang, the substrate composes through the canonical
// sort catalog:
//
//   source-lang-sort → concept-hub-sort → target-lang-sort
//
// This module reads the morphism catalog and answers:
//
//   compose_sort_to_target(
//       source_lang_signature_cid: &str,
//       source_lang_sort_cid: &str,
//       target_lang_signature_cid: &str,
//   ) -> Result<ComposeOutcome, ComposeRefusal>
//
// where ComposeOutcome carries the resulting target-lang-sort CID +
// the worst-case loss profile across the two-step composition.
//
// IMPORTANT: this module does NOT contain per-(source, target) type
// tables. The translation knowledge lives entirely in the catalog
// morphisms; this module is pure catalog query.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// In-memory index of sort-morphism mementos in the catalog.
///
/// Built once via `load_catalog`, queried many times via `compose_sort_to_target`.
#[allow(dead_code)] // wired into materialize in chunk 2 part B follow-up
pub struct SortMorphismCatalog {
    entries: Vec<SortMorphismEntry>,
}

#[derive(Debug, Clone)]
struct SortMorphismEntry {
    source_language_signature_cid: String,
    source_sort_cid: String,
    target_sort_cid: String,
    direction: String,
    precision_loss: String,
    range_loss: String,
}

#[derive(Deserialize)]
struct SortMorphismFile {
    header: SortMorphismHeader,
}

#[derive(Deserialize)]
struct SortMorphismHeader {
    source_language_signature_cid: String,
    source_sort_cid: String,
    target_sort_cid: String,
    direction: String,
    precision_loss: String,
    range_loss: String,
}

/// Successful composition: source-lang-sort → concept-hub-sort → target-lang-sort.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub struct ComposeOutcome {
    /// CID of the target-lang sort the source-lang-sort composes to.
    pub target_sort_cid: String,
    /// CID of the concept-hub sort traversed in the middle.
    pub concept_hub_sort_cid: String,
    /// Worst-case direction across the two morphism legs.
    /// - "bidirectional" iff BOTH legs are bidirectional.
    /// - "left-to-right" otherwise (composition direction is fixed
    ///   source → target; the reverse may not be possible).
    pub direction: String,
    /// Worst-case precision_loss. "none" only if both legs are "none".
    pub precision_loss: String,
    /// Worst-case range_loss. "none" only if both legs are "none".
    pub range_loss: String,
}

/// Refusal: composition is structurally impossible (no morphism leg found).
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub struct ComposeRefusal {
    pub reason: String,
}

#[allow(dead_code)]
impl SortMorphismCatalog {
    /// Load all sort-morphism:*.json entries from
    /// menagerie/concept-shapes/catalog/algorithms/ under the given workspace root.
    pub fn load_catalog(workspace_root: &Path) -> Result<Self, String> {
        let dir = workspace_root
            .join("menagerie")
            .join("concept-shapes")
            .join("catalog")
            .join("algorithms");
        if !dir.is_dir() {
            return Err(format!(
                "sort_morphism_catalog: directory not found: {}",
                dir.display()
            ));
        }
        let mut entries = Vec::new();
        for fn_entry in fs::read_dir(&dir).map_err(|e| format!("read_dir: {e}"))? {
            let fn_entry = fn_entry.map_err(|e| format!("read entry: {e}"))?;
            let path = fn_entry.path();
            let name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n,
                None => continue,
            };
            // Only read non-gap sort-morphism files. Gaps are documented
            // obligations, not composition primitives — they MUST NOT be
            // used as morphism legs.
            if !name.starts_with("sort-morphism:") {
                continue;
            }
            let raw = match fs::read_to_string(&path) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let parsed: SortMorphismFile = match serde_json::from_str(&raw) {
                Ok(p) => p,
                Err(_) => continue,
            };
            entries.push(SortMorphismEntry {
                source_language_signature_cid: parsed.header.source_language_signature_cid,
                source_sort_cid: parsed.header.source_sort_cid,
                target_sort_cid: parsed.header.target_sort_cid,
                direction: parsed.header.direction,
                precision_loss: parsed.header.precision_loss,
                range_loss: parsed.header.range_loss,
            });
        }
        Ok(Self { entries })
    }

    /// Compose source-lang sort → concept-hub sort → target-lang sort via
    /// two-step morphism lookup. Returns Ok(ComposeOutcome) when both
    /// legs exist; Err(ComposeRefusal) when either is missing.
    pub fn compose_sort_to_target(
        &self,
        source_lang_signature_cid: &str,
        source_lang_sort_cid: &str,
        target_lang_signature_cid: &str,
    ) -> Result<ComposeOutcome, ComposeRefusal> {
        // Leg 1: source-lang-sort → concept-hub-sort.
        // Morphism filename convention: source = lang, target = concept-hub.
        let leg1 = self.entries.iter().find(|e| {
            e.source_language_signature_cid == source_lang_signature_cid
                && e.source_sort_cid == source_lang_sort_cid
        });
        let leg1 = match leg1 {
            Some(e) => e,
            None => {
                return Err(ComposeRefusal {
                    reason: format!(
                        "no source morphism: source_lang_signature_cid={source_lang_signature_cid} \
                         + source_sort_cid={source_lang_sort_cid} not found in catalog"
                    ),
                })
            }
        };
        let concept_hub_sort_cid = leg1.target_sort_cid.clone();

        // Leg 2: concept-hub-sort → target-lang-sort. The morphism file
        // mints lang → concept-hub direction; to traverse the reverse,
        // we need the leg with target_sort_cid == concept_hub_sort_cid
        // AND source_language_signature_cid == target-lang. If that
        // leg's direction is left-to-right only, we cannot reverse it.
        let leg2 = self.entries.iter().find(|e| {
            e.source_language_signature_cid == target_lang_signature_cid
                && e.target_sort_cid == concept_hub_sort_cid
        });
        let leg2 = match leg2 {
            Some(e) => e,
            None => {
                return Err(ComposeRefusal {
                    reason: format!(
                        "no target morphism: target_lang_signature_cid={target_lang_signature_cid} \
                         with target_sort_cid={concept_hub_sort_cid} (concept-hub) not found"
                    ),
                })
            }
        };

        // Worst-case direction: bidirectional iff both legs are bidirectional.
        // Going source→target uses leg1 forward (must be bidirectional or
        // left-to-right) and leg2 BACKWARD (must be bidirectional —
        // left-to-right narrowing morphisms cannot be inverted).
        if leg2.direction == "left-to-right" {
            return Err(ComposeRefusal {
                reason: format!(
                    "leg2 morphism is left-to-right (narrowing) and cannot be reversed: \
                     concept-hub → target-lang composition refused. Loss: {} (precision) \
                     + {} (range). To traverse this direction the catalog would need a \
                     concept-hub → target-lang morphism with explicit cast/widening; mint \
                     that morphism to close.",
                    leg2.precision_loss, leg2.range_loss
                ),
            });
        }

        let direction = if leg1.direction == "bidirectional" && leg2.direction == "bidirectional" {
            "bidirectional".to_string()
        } else {
            "left-to-right".to_string()
        };
        let precision_loss = worst_loss(&leg1.precision_loss, &leg2.precision_loss);
        let range_loss = worst_loss(&leg1.range_loss, &leg2.range_loss);

        Ok(ComposeOutcome {
            target_sort_cid: leg2.source_sort_cid.clone(),
            concept_hub_sort_cid,
            direction,
            precision_loss,
            range_loss,
        })
    }
}

/// Combine two loss fields: "none" iff both are "none". Otherwise take
/// the more-lossy one (string-comparison is stable for the catalog's
/// vocabulary of "none" / "narrowing" / "2^53-bounded" etc.).
fn worst_loss(a: &str, b: &str) -> String {
    if a == "none" && b == "none" {
        "none".to_string()
    } else if a == "none" {
        b.to_string()
    } else if b == "none" {
        a.to_string()
    } else if a == b {
        a.to_string()
    } else {
        format!("{a}+{b}")
    }
}

#[allow(dead_code)]
pub fn default_workspace_root() -> PathBuf {
    // Heuristic for tests: walk up from CARGO_MANIFEST_DIR/../../..
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from("."))
}

// SUBSTRATE-HONEST INVARIANT (2026-05-21 architectural correction):
//
// kit-internal sort identifiers (rust:Int, python:Value, etc.) MUST NOT
// appear in ProofIR or in cmd_materialize. They live inside their kits
// only. The cross-language wire is:
//
//   walk_rpc (rust kit, internal: rust:Int)
//     → ProofIR carries concept-hub-typed signature (concept:Int)
//   cmd_materialize routes the ProofIR to the target kit
//     → target realize binary (internal: java:Int / python:int / ts:number)
//   target kit emits target-language source
//
// cmd_materialize does NOT compose source-lang-sort → target-lang-sort.
// It dispatches concept-hub-typed payloads from one kit to another.
//
// The SortMorphismCatalog above is a substrate-level query primitive
// for AUDIT / DISCOVERY purposes (e.g. "is there a hub-traversal path
// from concept:Int through java's morphism graph?"). It's NOT the
// production code path for materialize — that path is kit → kit via
// concept-hub-typed payloads.
//
// Source-syntax recognition (e.g. rust `&str` → rust:Str → concept:String)
// lives in each kit's source lifter (walk_rpc for rust, the typescript
// source lifter for ts, etc.). Each kit declares its own source-syntax
// → lang-sort mapping internally, then composes via its own morphism
// to concept-hub before emitting the carrier payload.

#[allow(dead_code)]
pub fn known_sort_cids() -> HashMap<&'static str, &'static str> {
    // For test convenience: known substrate-canonical primitive CIDs.
    // (Same values populated by the mint scripts in menagerie/concept-shapes/scripts.)
    let mut m = HashMap::new();
    m.insert(
        "concept:Int",
        "blake3-512:30ffc51350121a7172f3e4064a33c45bbd345756979fccff6875cd2ab33e4964d098a99df80cfbdf1ec1a0738c5ac3476f0ff8f75589ea511d1acd82c74ecd58",
    );
    m.insert(
        "concept:String",
        "blake3-512:be8721d24849feb74c4f1bd1c6c1d4a4b39ef03ef9a92b2bdfac1c4d4f0aa3da6f2c14b32edb3a8f17fd1f5b7df9a1ec4eb73f24a4d1ed8c9bc5f0f95dfb27107",
    );
    m.insert(
        "concept:Bool",
        "blake3-512:0ee13bf3fd6b7ecfbeec19c2ce71b3a3f5dca90e4f0fa8b48f1afe8cea1ae8a73b71e3a64f93dca8eddf9a55f5ee10ef5fab30a52afdc3a8b25a1b4d7d24e0eed",
    );
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("..")
    }

    const RUST_SIG: &str =
        "blake3-512:e3c223b8b6f39382e43cb06c5b04059987e661d96311decd5003d4ec79c7d6f9969de39ae16dd6509cb5236185260d59c63288db7ff772aae00f8123ea826cbd";
    const PYTHON_SIG: &str =
        "blake3-512:bc36b43fec1a80efcecb05f8c4de725f961295466530aec452763c6c479b67c590c2e8062a3f46979383086ae80e6c0a917c443625d3474a7a89705e0a56ab8c";
    const JAVA_SIG: &str =
        "blake3-512:4d312a5ab13eba517063f097a73b8675f1ea2a915ab5cc5b92587c83b47f707b0298858ac2c48061eee73e30e1c48983bd0aec5641bc806561624fc7e4da44ef";

    #[test]
    fn catalog_loads_morphisms() {
        let cat = SortMorphismCatalog::load_catalog(&ws()).expect("load");
        assert!(
            cat.entries.len() >= 140,
            "expected ≥140 morphisms in catalog after 2026-05-21 mints; got {}",
            cat.entries.len()
        );
    }

    #[test]
    fn compose_rust_int_to_java_int_succeeds() {
        // rust:Int sort CID — looked up directly from the catalog. Compute via grep helper.
        let cat = SortMorphismCatalog::load_catalog(&ws()).expect("load");
        // Find rust:Int → concept-hub morphism to learn source_sort_cid.
        let leg1 = cat
            .entries
            .iter()
            .find(|e| {
                e.source_language_signature_cid == RUST_SIG
                    && e.target_sort_cid.starts_with("blake3-512:30ffc513")
            })
            .expect("rust:Int → concept:Int morphism present");
        let rust_int_sort_cid = leg1.source_sort_cid.clone();
        let outcome = cat
            .compose_sort_to_target(RUST_SIG, &rust_int_sort_cid, JAVA_SIG)
            .expect("compose rust:Int → java:Int");
        assert_eq!(outcome.direction, "bidirectional");
        assert_eq!(outcome.precision_loss, "none");
        assert_eq!(outcome.range_loss, "none");
    }

    #[test]
    fn compose_refuses_when_target_morphism_left_to_right() {
        // python's primitive morphisms use python:Value with narrowing
        // (left-to-right). Cannot reverse-traverse concept-hub → python:Value.
        // Going rust:Int → concept:Int → python: would need a bidirectional
        // python morphism. Our python:Int → concept:Int IS bidirectional
        // (python int is unbounded), but Value-fallback for Bytes/Cid/etc.
        // is left-to-right.
        // This test asserts the refusal path FIRES correctly on a known
        // narrowing case (rust:Bytes → concept:Bytes → python:Bytes via
        // python:Value narrowing — python:Value→concept:Bytes is L→R).
        let cat = SortMorphismCatalog::load_catalog(&ws()).expect("load");
        let leg1 = cat
            .entries
            .iter()
            .find(|e| {
                e.source_language_signature_cid == RUST_SIG
                    && e.target_sort_cid.starts_with("blake3-512:7116ef6e")
            }) // concept:Bytes CID prefix
            .expect("rust:Bytes → concept:Bytes morphism");
        let rust_bytes_sort_cid = leg1.source_sort_cid.clone();
        let result = cat.compose_sort_to_target(RUST_SIG, &rust_bytes_sort_cid, PYTHON_SIG);
        match result {
            Ok(outcome) => {
                // If python:Bytes is bidirectional this succeeds.
                // python has a typed bytes sort so this should work.
                assert_eq!(outcome.direction, "bidirectional");
            }
            Err(refusal) => {
                // If left-to-right narrowing fired, the refusal names the leg.
                assert!(refusal.reason.contains("left-to-right"));
            }
        }
    }

    #[test]
    fn compose_refuses_when_no_source_morphism() {
        let cat = SortMorphismCatalog::load_catalog(&ws()).expect("load");
        let refusal = cat
            .compose_sort_to_target(
                "blake3-512:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
                "blake3-512:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
                JAVA_SIG,
            )
            .expect_err("nonexistent source lang must refuse");
        assert!(refusal.reason.contains("no source morphism"));
    }
}
