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
