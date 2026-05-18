// SPDX-License-Identifier: Apache-2.0
//
// Binding-kit declaration for the TypeScript `pg` library (node-postgres).
//
// This is a BINDING-kit declaration (not a language-kit). It characterizes
// how the `pg` library exposes the result of an INSERT that returns the
// newly created row id.
//
// Binding tag: "pg"  (the tag component after splitting "typescript-pg"
// via split_library_surface).
//
// RowIdMechanism = ReturningClause: PostgreSQL requires appending a
// `RETURNING id` (or equivalent) clause to the INSERT statement. The row id
// is returned as a column in the result set, not as connection-global state.
// This mechanism requires a structural rewrite of the INSERT SQL string
// relative to better-sqlite3, making it the load-bearing divergence
// characterization for the typescript-better-sqlite3 -> typescript-pg
// migration walk.
//
// Concept-op CID for concept:insert-and-get-id: (same as better_sqlite3.rs)
//   blake3-512:0a4f0a8d36d8dee96b8d5b32a18bb390f35877ecef611771048c6e10cfc3d25ad8f59de89b00c7794f62cabaf91dbd779244338393a8bb6ef5e8309b0929b3ca

use std::collections::BTreeMap;

use provekit_canonicalizer::blake3_512_of;
use provekit_ir_types::{DimensionValueMemento, IrFormula, IrTerm, PlatformSemanticTag};

use crate::core::types::PlatformSemanticsDeclaration;

use super::better_sqlite3::CONCEPT_INSERT_AND_GET_ID_CID;

const KIT_ID: &str = "provekit-binding-pg@0.1.0";

pub fn declaration() -> PlatformSemanticsDeclaration {
    let kit_cid = blake3_512_of(KIT_ID.as_bytes());
    let values = dimension_values_for_kit(&kit_cid);
    let value_cids = values
        .iter()
        .map(|v| (v.value_name.clone(), v.cid.clone()))
        .collect::<BTreeMap<_, _>>();

    PlatformSemanticsDeclaration {
        tags: vec![tag(
            &kit_cid,
            CONCEPT_INSERT_AND_GET_ID_CID,
            &[(
                "RowIdMechanism",
                value_cids["ReturningClause"].as_str(),
            )],
        )],
        dimension_values: values,
        op_aliases: BTreeMap::new(),
    }
}

pub fn dimension_values() -> Vec<DimensionValueMemento> {
    let kit_cid = blake3_512_of(KIT_ID.as_bytes());
    dimension_values_for_kit(&kit_cid)
}

fn dimension_values_for_kit(kit_cid: &str) -> Vec<DimensionValueMemento> {
    vec![
        // ReturningClause: row id is sourced from a server-side RETURNING clause
        // appended to the INSERT SQL statement. The clause causes the server to
        // return the specified columns (including the id) as a result row. The
        // compare_to formula is an atomic predicate whose `source` arg names
        // the access mechanism: a Ctor term "returning_clause_result" applied to
        // "insert_result_set_at_call_return". Structurally distinct from
        // LastInsertRowid (which uses "last_insert_rowid" applied to
        // "connection_state_at_call_return").
        DimensionValueMemento::new(
            kit_cid.to_string(),
            "RowIdMechanism".to_string(),
            "ReturningClause".to_string(),
            IrFormula::Atomic {
                name: "row_id_source".to_string(),
                args: vec![IrTerm::Ctor {
                    name: "returning_clause_result".to_string(),
                    args: vec![IrTerm::Ctor {
                        name: "insert_result_set_at_call_return".to_string(),
                        args: vec![],
                    }],
                }],
            },
        ),
    ]
}

fn tag(kit_cid: &str, op_cid: &str, pairs: &[(&str, &str)]) -> PlatformSemanticTag {
    let mut dimensions = BTreeMap::new();
    for (dimension, cid) in pairs {
        dimensions.insert((*dimension).to_string(), (*cid).to_string());
    }
    PlatformSemanticTag::new(kit_cid.to_string(), op_cid.to_string(), dimensions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::better_sqlite3;

    // Positive: declaration is non-empty (has insert-and-get-id tag with ReturningClause)
    #[test]
    fn pg_declaration_is_non_empty() {
        let decl = declaration();
        assert!(
            !decl.tags.is_empty(),
            "pg binding kit must declare at least one op tag"
        );
        assert!(
            decl.tags.iter().any(|t| t.op_cid == CONCEPT_INSERT_AND_GET_ID_CID),
            "pg kit must declare concept:insert-and-get-id"
        );
        assert!(
            !decl.dimension_values.is_empty(),
            "pg kit must declare dimension values"
        );
    }

    // Discrimination: pg uses ReturningClause; better-sqlite3 uses LastInsertRowid.
    // Pairing them on concept:insert-and-get-id must show a dimension divergence.
    #[test]
    fn pg_row_id_mechanism_differs_from_better_sqlite3() {
        let pg_decl = declaration();
        let sqlite_decl = better_sqlite3::declaration();

        let pg_tag = pg_decl
            .tags
            .iter()
            .find(|t| t.op_cid == CONCEPT_INSERT_AND_GET_ID_CID)
            .expect("pg must have insert-and-get-id tag");
        let sqlite_tag = sqlite_decl
            .tags
            .iter()
            .find(|t| t.op_cid == CONCEPT_INSERT_AND_GET_ID_CID)
            .expect("better-sqlite3 must have insert-and-get-id tag");

        let pg_mechanism = pg_tag
            .dimensions
            .get("RowIdMechanism")
            .expect("pg tag must have RowIdMechanism dimension");
        let sqlite_mechanism = sqlite_tag
            .dimensions
            .get("RowIdMechanism")
            .expect("better-sqlite3 tag must have RowIdMechanism dimension");

        assert_ne!(
            pg_mechanism, sqlite_mechanism,
            "ReturningClause and LastInsertRowid must hash to different dimension value CIDs"
        );

        // compare_op_with must report a divergence between the two declarations
        use crate::core::types::OpCoverageVerdict;
        let verdict = sqlite_decl
            .compare_op_with(CONCEPT_INSERT_AND_GET_ID_CID, &pg_decl)
            .expect("compare_op_with must not error for a declared op");
        assert!(
            matches!(verdict, OpCoverageVerdict::Divergent(_)),
            "comparing sqlite vs pg on insert-and-get-id must yield OpCoverageVerdict::Divergent"
        );
        if let OpCoverageVerdict::Divergent(div) = verdict {
            assert_eq!(div.dimension_name, "RowIdMechanism");
        }
    }

    // Structural: the ReturningClause dimension value carries a valid IrFormula compare_to.
    #[test]
    fn pg_dimension_value_has_valid_compare_to() {
        let values = dimension_values();
        let row_id_value = values
            .iter()
            .find(|v| v.dimension_name == "RowIdMechanism" && v.value_name == "ReturningClause")
            .expect("must have ReturningClause dimension value");

        // The compare_to must be an Atomic with non-empty args (not a bare name-only Atomic)
        assert!(
            matches!(&row_id_value.compare_to, IrFormula::Atomic { args, .. } if !args.is_empty()),
            "ReturningClause compare_to must be an IrFormula::Atomic with IrTerm args"
        );
        // CID must be a valid blake3-512 string
        assert!(
            row_id_value.cid.starts_with("blake3-512:"),
            "dimension value CID must start with blake3-512:"
        );
        assert_eq!(
            row_id_value.cid.len(),
            139,
            "dimension value CID must be 139 chars"
        );
    }
}
