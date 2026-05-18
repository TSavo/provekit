// SPDX-License-Identifier: Apache-2.0
//
// Binding-kit declaration for the TypeScript `better-sqlite3` library.
//
// This is a BINDING-kit declaration (not a language-kit). It characterizes
// how the `better-sqlite3` library exposes the result of an INSERT that
// returns the newly created row id.
//
// Binding tag: "better-sqlite3"  (the tag component after splitting
// "typescript-better-sqlite3" via split_library_surface).
//
// RowIdMechanism = LastInsertRowid: the library exposes the inserted row id
// as `stmt.run(...).lastInsertRowid`, which reads connection-global mutable
// state maintained by SQLite and set by the most recent INSERT on that
// connection. This is structurally different from the PostgreSQL mechanism
// (RETURNING clause) and requires a distinct migration pattern.
//
// Concept-op CID for concept:insert-and-get-id:
//   blake3-512:0a4f0a8d36d8dee96b8d5b32a18bb390f35877ecef611771048c6e10cfc3d25ad8f59de89b00c7794f62cabaf91dbd779244338393a8bb6ef5e8309b0929b3ca
// Minted from the AlgorithmMemento JSON in:
//   menagerie/concept-shapes/catalog/algorithms/concept:insert-and-get-id.<cid>.json

use std::collections::BTreeMap;

use provekit_canonicalizer::blake3_512_of;
use provekit_ir_types::{DimensionValueMemento, IrFormula, IrTerm, PlatformSemanticTag};

use crate::core::types::PlatformSemanticsDeclaration;

const KIT_ID: &str = "provekit-binding-better-sqlite3@0.1.0";

/// CID for concept:insert-and-get-id, minted from its AlgorithmMemento via JCS+blake3-512.
pub const CONCEPT_INSERT_AND_GET_ID_CID: &str =
    "blake3-512:0a4f0a8d36d8dee96b8d5b32a18bb390f35877ecef611771048c6e10cfc3d25ad8f59de89b00c7794f62cabaf91dbd779244338393a8bb6ef5e8309b0929b3ca";

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
                value_cids["LastInsertRowid"].as_str(),
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
        // LastInsertRowid: row id is sourced from connection-global mutable state
        // (SQLite's last_insert_rowid field on the connection) set by the most
        // recent INSERT. The compare_to formula is an atomic predicate whose
        // `source` arg names the access mechanism: a Ctor term "last_insert_rowid"
        // applied to "connection_state_at_call_return". This structurally
        // distinguishes it from the ReturningClause mechanism (which uses a
        // "returning_clause_result" Ctor applied to "insert_result_set_at_call_return").
        DimensionValueMemento::new(
            kit_cid.to_string(),
            "RowIdMechanism".to_string(),
            "LastInsertRowid".to_string(),
            IrFormula::Atomic {
                name: "row_id_source".to_string(),
                args: vec![IrTerm::Ctor {
                    name: "last_insert_rowid".to_string(),
                    args: vec![IrTerm::Ctor {
                        name: "connection_state_at_call_return".to_string(),
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

    // Positive: declaration is non-empty (has at least one tag for insert-and-get-id)
    #[test]
    fn better_sqlite3_declaration_is_non_empty() {
        let decl = declaration();
        assert!(
            !decl.tags.is_empty(),
            "better-sqlite3 binding kit must declare at least one op tag"
        );
        assert!(
            decl.tags.iter().any(|t| t.op_cid == CONCEPT_INSERT_AND_GET_ID_CID),
            "better-sqlite3 kit must declare concept:insert-and-get-id"
        );
        assert!(
            !decl.dimension_values.is_empty(),
            "better-sqlite3 kit must declare dimension values"
        );
    }

    // Discrimination: better-sqlite3 uses LastInsertRowid; pg uses ReturningClause.
    // The two kits must produce different RowIdMechanism dimension value CIDs.
    #[test]
    fn better_sqlite3_row_id_mechanism_differs_from_pg() {
        use super::super::pg;
        let sqlite_decl = declaration();
        let pg_decl = pg::declaration();

        let sqlite_tag = sqlite_decl
            .tags
            .iter()
            .find(|t| t.op_cid == CONCEPT_INSERT_AND_GET_ID_CID)
            .expect("better-sqlite3 must have insert-and-get-id tag");
        let pg_tag = pg_decl
            .tags
            .iter()
            .find(|t| t.op_cid == CONCEPT_INSERT_AND_GET_ID_CID)
            .expect("pg must have insert-and-get-id tag");

        let sqlite_mechanism = sqlite_tag
            .dimensions
            .get("RowIdMechanism")
            .expect("better-sqlite3 tag must have RowIdMechanism dimension");
        let pg_mechanism = pg_tag
            .dimensions
            .get("RowIdMechanism")
            .expect("pg tag must have RowIdMechanism dimension");

        assert_ne!(
            sqlite_mechanism, pg_mechanism,
            "LastInsertRowid and ReturningClause must hash to different dimension value CIDs"
        );
    }

    // Structural: the RowIdMechanism dimension value carries a valid IrFormula compare_to.
    #[test]
    fn better_sqlite3_dimension_value_has_valid_compare_to() {
        let values = dimension_values();
        let row_id_value = values
            .iter()
            .find(|v| v.dimension_name == "RowIdMechanism" && v.value_name == "LastInsertRowid")
            .expect("must have LastInsertRowid dimension value");

        // The compare_to must be an Atomic with non-empty args to structurally
        // distinguish it from a plain name-only Atomic.
        assert!(
            matches!(&row_id_value.compare_to, IrFormula::Atomic { args, .. } if !args.is_empty()),
            "LastInsertRowid compare_to must be an IrFormula::Atomic with IrTerm args"
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
