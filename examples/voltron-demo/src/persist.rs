// SPDX-License-Identifier: Apache-2.0
//
// persist.rs — SQL persistence module. Three boundary stubs for rusqlite
// (open_in_memory, sql_execute, sql_query_row_string) + user-side
// install_schema and insert_event functions. The cross-vendor seam STARTS
// here: insert_event receives a ValidEvent (string fields from JSON
// ingest) and binds them into SQL parameters — json's post must
// establish sql's pre for the spine to discharge.

use crate::ingest::ValidEvent;
use rusqlite::Connection;

// =============================================================================
// Boundary: concept:sql-connection-open  →  provekit-shim-rusqlite
// =============================================================================
// provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:sql-connection-open","function":"open_in_memory","params":[],"param_types":[],"return_type":"rusqlite::Result<Connection>","named_term_tree":{"conceptName":"concept:sql-connection-open","args":[]}}
// provekit-concept-payload-cid: blake3-512:ab283460c93774b8edad9ffde9b9861d004c81bd78e627b45f70efd18ea8190a860628b7a4dd69a1668366b4fbe8c78f7ff709f7fc44b550e7b54916340156e3
pub fn open_in_memory() -> rusqlite::Result<Connection> {
    unimplemented!("provekit materialize fills this from provekit-shim-rusqlite")
}

// =============================================================================
// Boundary: concept:sql-execute  →  provekit-shim-rusqlite
// =============================================================================
// provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:sql-execute","function":"sql_execute","params":["conn","sql","args"],"param_types":["&Connection","&str","&[&dyn rusqlite::ToSql]"],"return_type":"rusqlite::Result<usize>","named_term_tree":{"conceptName":"concept:sql-execute","args":[{"sort":"SqlConnection","source":"conn"},{"sort":"Sql","source":"sql"},{"sort":"SqlArgs","source":"args"}]}}
// provekit-concept-payload-cid: blake3-512:235e3796f2337195061175717a5488392d7c5268f3747355e7b9c5e7254d6e8360f7f0bfbca5db50f53da0807d30f6f8fd9151b4a0d45630b4de29f961f91509
pub fn sql_execute(
    _conn: &Connection,
    _sql: &str,
    _args: &[&dyn rusqlite::ToSql],
) -> rusqlite::Result<usize> {
    unimplemented!("provekit materialize fills this from provekit-shim-rusqlite")
}

// =============================================================================
// Boundary: concept:sql-query-row  →  provekit-shim-rusqlite
// (typed as String; persist's row-shape post → report's json-parse pre)
// =============================================================================
// provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:sql-query-row","function":"sql_query_row_string","params":["conn","sql","args"],"param_types":["&Connection","&str","&[&dyn rusqlite::ToSql]"],"return_type":"rusqlite::Result<String>","named_term_tree":{"conceptName":"concept:sql-query-row","args":[{"sort":"SqlConnection","source":"conn"},{"sort":"Sql","source":"sql"},{"sort":"SqlArgs","source":"args"}]}}
// provekit-concept-payload-cid: blake3-512:312002201df4701b9c7b3618e1300ce8ab708425f7b73333b206aa99fb8d55f70e7e2e18471189da339e584ab44a17d9e09ae954dc46e8055e499bc3f7edd16c
pub fn sql_query_row_string(
    _conn: &Connection,
    _sql: &str,
    _args: &[&dyn rusqlite::ToSql],
) -> rusqlite::Result<String> {
    unimplemented!("provekit materialize fills this from provekit-shim-rusqlite")
}

// =============================================================================
// User-side function: install_schema.
// Lifter-derived contract:
//   pre:  conn is an open connection (rust type system)
//   post: events table exists with columns (id, type, user, payload)
// =============================================================================

pub fn install_schema(conn: &Connection) -> rusqlite::Result<()> {
    sql_execute(
        conn,
        "CREATE TABLE events (id INTEGER PRIMARY KEY, type TEXT NOT NULL, user TEXT NOT NULL, payload TEXT NOT NULL)",
        &[],
    )?;
    Ok(())
}

// =============================================================================
// User-side function: insert_event.
// Lifter-derived contract:
//   pre:  conn has events table (established by install_schema's post)
//         event.user is non-empty (established by parse_event's post)
//   post: result is the inserted rowid > 0 (the panic on rowid<=0
//         guard lifts to the post-condition)
//
// Cross-vendor seam: event.event_type / event.user / event.payload_text
// are JSON-vendor outputs; here they become SQL-vendor inputs as
// rusqlite::ToSql trait objects. Substrate discharges that morphism.
// =============================================================================

pub fn insert_event(conn: &Connection, event: &ValidEvent) -> rusqlite::Result<i64> {
    let event_type: &dyn rusqlite::ToSql = &event.event_type;
    let user: &dyn rusqlite::ToSql = &event.user;
    let payload: &dyn rusqlite::ToSql = &event.payload_text;
    sql_execute(
        conn,
        "INSERT INTO events (type, user, payload) VALUES (?1, ?2, ?3)",
        &[event_type, user, payload],
    )?;
    let rowid_str = sql_query_row_string(conn, "SELECT last_insert_rowid()", &[])?;
    let rowid: i64 = rowid_str
        .parse()
        .map_err(|_| rusqlite::Error::InvalidQuery)?;
    if rowid <= 0 {
        panic!("persist: SQL INTEGER PRIMARY KEY post violated — rowid <= 0");
    }
    Ok(rowid)
}
