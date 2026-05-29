// SPDX-License-Identifier: Apache-2.0
//
// Seed concept-shape catalog used by the smoke-test driver.
//
// In production, this catalog is the menagerie/concept-shapes/catalog
// directory (signed shape mementos with associated wp_rules). The smoke
// test ships a tiny in-binary stand-in so the demo is reproducible
// without first wiring the live catalog into the binary. The catalog
// entries are matched in two passes:
//
//   1. Hard match: the shape-CID lookup. If a catalog entry has the
//      exact shape-CID, the binding inherits the catalog name AND any
//      registered wp_rule.
//
//   2. Soft match: classify the shape (retry-loop, guard-then-commit,
//      option-default, ...) and match by classification. Soft matches
//      attach a name but do NOT mint a contract on their own; the
//      synthesize::wp_rule_for_shape step decides that separately so
//      we never accidentally fabricate semantics from a name match.

use crate::algebra::TermShape;
use provekit_ir_types::{
    GapKind, OptionStatus, ResolutionOption, ResolutionOptionKind, TransportGapMemento,
};

#[derive(Debug, Clone)]
pub struct CatalogEntry {
    pub id: String,
    pub name: String,
    pub classification: &'static str,
}

#[derive(Debug, Clone)]
pub struct Catalog {
    pub entries: Vec<CatalogEntry>,
}

impl Catalog {
    pub fn match_shape(&self, _shape_cid: &str, shape: &TermShape) -> Option<&CatalogEntry> {
        // The smoke-test's hard-CID lookup is empty (no signed catalog
        // bundle is loaded into this binary). We do a classification-
        // only soft match for the demo. Production loads signed
        // ConceptAbstractionMementos from menagerie/concept-shapes/.
        let cls = shape.classify();
        if cls == "unknown" {
            return None;
        }
        self.entries.iter().find(|e| e.classification == cls)
    }
}

pub fn seed_catalog() -> Catalog {
    Catalog {
        entries: vec![
            CatalogEntry {
                id: "shape:retry-with-bounded-attempts".into(),
                name: "concept:retry-with-bounded-attempts".into(),
                classification: "retry-loop",
            },
            CatalogEntry {
                id: "shape:guard-then-commit".into(),
                name: "concept:guard-then-commit".into(),
                classification: "guard-then-commit",
            },
            CatalogEntry {
                id: "shape:option-default".into(),
                name: "concept:option-default".into(),
                classification: "option-default",
            },
        ],
    }
}

pub fn unknown_shape_gap_record(
    shape_cid: &str,
    concept: &str,
    language: &str,
) -> Result<serde_json::Value, String> {
    let gap = TransportGapMemento {
        fn_name: format!(
            "gap:{}:cluster:unknown-shape:to:{}",
            language,
            concept.trim_start_matches("concept:")
        ),
        gap_kind: GapKind::MissingTargetConstruct,
        kind: "TransportGapMemento".to_string(),
        reason: None,
        reason_note: Some("cluster refused an unknown shape without a catalog concept".to_string()),
        resolution_options: vec![ResolutionOption {
            dual_view_cid: None,
            loss: None,
            loss_severity: None,
            option_kind: ResolutionOptionKind::AcceptPermanent,
            partial_morphism_cid: None,
            precondition: None,
            representation_map_delta: None,
            respec_target_to: None,
            split_targets: None,
            status: OptionStatus::Deferred,
            tradeoff:
                "name the concept or add a catalog shape before treating the cluster as exact"
                    .to_string(),
        }],
        schema_version: "1".to_string(),
        signature: None,
        source_lang: language.to_string(),
        source_op_cid: shape_cid.to_string(),
        target_concept_op: concept.to_string(),
        target_op_cid: None,
    };
    serde_json::to_value(gap).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_shape_gap_does_not_emit_unrelated_gap_variant() {
        let gap = unknown_shape_gap_record(
            "blake3-512:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "concept:add",
            "rust",
        )
        .expect("gap serializes");

        assert_eq!(gap["gap_kind"], "missing-target-construct");
        assert_ne!(gap["gap_kind"], "wp-rule-mismatch");
    }
}
