// SPDX-License-Identifier: Apache-2.0
//
// provekit-shim-rusqlite: substrate-honest concept bindings for the rusqlite
// Rust SQLite driver.
//
// This crate is the first vendored boundary namespace under the ProvekIt
// proofchain (paper 03: substrate, not blockchain; paper 24: the proofchain
// is the exchange). Every claim this kit makes is in this file. There are
// no sidecar files. The substrate-uniform pattern is: the lift kit reads
// this source, extracts the structural shape of each annotated function
// body, attaches the per-binding `loss` declarations directly from the
// annotation arguments, attaches the `observed_dimension` for observation
// bindings, and emits `refusal-memento` IR for each `#[provekit::refuse]`
// attribute. cmd_mint consumes the lift kit's IR over JSON-RPC and produces
// a signed `.proof` envelope. No format, file path, or declaration in this
// crate exists outside what the lift kit reads from this source.
//
// Three speech acts per paper 24:
//   1. `#[provekit::sugar(... loss = [])]`           materialize
//   2. `#[provekit::sugar(... loss = [<dims>])]`     loudly-bounded-lossy
//   3. `#[provekit::refuse(...)]`                    refuse with reason
//
// Concept names are vendored under this kit's signature. Other kits joining
// the cluster cite the same names; the substrate recognizes the cluster by
// structural match on the term_shape carried in each binding's IR. There is
// no central concept-spec authoring step: the cluster IS the concept (paper
// 24 §2, paper 21 §6 Authored path).

pub use rusqlite::{
    Connection, OpenFlags, Params, Result, Row, Rows, Statement, Transaction,
    TransactionBehavior,
};

use std::path::Path;
use std::time::Duration;

// =============================================================================
// A. Connection lifecycle
// =============================================================================

#[provekit::sugar(
    concept = "concept:sql-connection-open",
    library = "rusqlite",
    loss = ["sync-vs-async", "auth-mechanism", "connection-pooling"],
)]
pub fn open<P: AsRef<Path>>(path: P) -> Result<Connection> {
    Connection::open(path)
}

#[provekit::sugar(
    concept = "concept:sql-connection-open",
    library = "rusqlite",
    loss = ["sync-vs-async", "persistence-target"],
)]
pub fn open_in_memory() -> Result<Connection> {
    Connection::open_in_memory()
}

#[provekit::sugar(
    concept = "concept:sql-connection-open",
    library = "rusqlite",
    loss = ["sync-vs-async", "auth-mechanism", "connection-pooling", "flag-encoding"],
)]
pub fn open_with_flags<P: AsRef<Path>>(path: P, flags: OpenFlags) -> Result<Connection> {
    Connection::open_with_flags(path, flags)
}

#[provekit::sugar(
    concept = "concept:sql-connection-close",
    library = "rusqlite",
    loss = ["sync-vs-async"],
)]
pub fn close(conn: Connection) -> std::result::Result<(), (Connection, rusqlite::Error)> {
    conn.close()
}

// =============================================================================
// B. Query execution at the Connection level
// =============================================================================

#[provekit::sugar(
    concept = "concept:sql-execute",
    library = "rusqlite",
    loss = ["sync-vs-async", "last-insert-id", "transaction-isolation"],
)]
pub fn execute<P: Params>(conn: &Connection, sql: &str, params: P) -> Result<usize> {
    conn.execute(sql, params)
}

#[provekit::sugar(
    concept = "concept:sql-batch-execute",
    library = "rusqlite",
    loss = ["sync-vs-async", "atomicity-across-statements", "parameter-binding"],
)]
pub fn execute_batch(conn: &Connection, sql: &str) -> Result<()> {
    conn.execute_batch(sql)
}

#[provekit::sugar(
    concept = "concept:sql-query",
    library = "rusqlite",
    loss = ["sync-vs-async", "row-cardinality", "mapper-side-effects"],
)]
pub fn query_row<T, P: Params, F: FnOnce(&Row<'_>) -> Result<T>>(
    conn: &Connection,
    sql: &str,
    params: P,
    mapper: F,
) -> Result<T> {
    conn.query_row(sql, params, mapper)
}

#[provekit::sugar(
    concept = "concept:sql-query",
    library = "rusqlite",
    loss = ["sync-vs-async", "row-cardinality", "mapper-side-effects", "error-composition-mode"],
)]
pub fn query_row_and_then<T, E, P, F>(
    conn: &Connection,
    sql: &str,
    params: P,
    mapper: F,
) -> std::result::Result<T, E>
where
    P: Params,
    E: From<rusqlite::Error>,
    F: FnOnce(&Row<'_>) -> std::result::Result<T, E>,
{
    conn.query_row_and_then(sql, params, mapper)
}

// =============================================================================
// C. Statement preparation
// =============================================================================

#[provekit::sugar(
    concept = "concept:sql-prepare",
    library = "rusqlite",
    loss = ["sync-vs-async", "parameter-style", "statement-lifetime"],
)]
pub fn prepare<'conn>(conn: &'conn Connection, sql: &str) -> Result<Statement<'conn>> {
    conn.prepare(sql)
}

#[provekit::sugar(
    concept = "concept:sql-prepare-cached",
    library = "rusqlite",
    loss = ["sync-vs-async", "cache-eviction-policy", "cache-size-bound"],
)]
pub fn prepare_cached<'conn>(
    conn: &'conn Connection,
    sql: &str,
) -> Result<rusqlite::CachedStatement<'conn>> {
    conn.prepare_cached(sql)
}

#[provekit::sugar(
    concept = "concept:sql-prepare-cached",
    library = "rusqlite",
    loss = ["cache-side-effect"],
)]
pub fn discard_cached(stmt: rusqlite::CachedStatement<'_>) {
    stmt.discard()
}

// =============================================================================
// D. Statement execution
// =============================================================================

#[provekit::sugar(
    concept = "concept:sql-execute",
    library = "rusqlite",
    loss = ["sync-vs-async", "last-insert-id", "transaction-isolation"],
)]
pub fn stmt_execute<P: Params>(stmt: &mut Statement<'_>, params: P) -> Result<usize> {
    stmt.execute(params)
}

#[provekit::sugar(
    concept = "concept:sql-query",
    library = "rusqlite",
    loss = ["sync-vs-async", "cursor-lifetime"],
)]
pub fn stmt_query<'stmt, P: Params>(
    stmt: &'stmt mut Statement<'_>,
    params: P,
) -> Result<Rows<'stmt>> {
    stmt.query(params)
}

#[provekit::sugar(
    concept = "concept:sql-query",
    library = "rusqlite",
    loss = ["sync-vs-async", "cursor-lifetime", "mapper-side-effects"],
)]
pub fn stmt_query_map<'stmt, T, P, F>(
    stmt: &'stmt mut Statement<'_>,
    params: P,
    mapper: F,
) -> Result<rusqlite::MappedRows<'stmt, F>>
where
    P: Params,
    F: FnMut(&Row<'_>) -> Result<T>,
{
    stmt.query_map(params, mapper)
}

#[provekit::sugar(
    concept = "concept:sql-query",
    library = "rusqlite",
    loss = ["sync-vs-async", "cursor-lifetime", "mapper-side-effects", "error-composition-mode"],
)]
pub fn stmt_query_and_then<'stmt, T, E, P, F>(
    stmt: &'stmt mut Statement<'_>,
    params: P,
    mapper: F,
) -> Result<rusqlite::AndThenRows<'stmt, F>>
where
    P: Params,
    E: From<rusqlite::Error>,
    F: FnMut(&Row<'_>) -> std::result::Result<T, E>,
{
    stmt.query_and_then(params, mapper)
}

#[provekit::sugar(
    concept = "concept:sql-query",
    library = "rusqlite",
    loss = ["sync-vs-async", "row-cardinality", "mapper-side-effects"],
)]
pub fn stmt_query_row<T, P, F>(stmt: &mut Statement<'_>, params: P, mapper: F) -> Result<T>
where
    P: Params,
    F: FnOnce(&Row<'_>) -> Result<T>,
{
    stmt.query_row(params, mapper)
}

#[provekit::sugar(
    concept = "concept:insert-and-get-id",
    library = "rusqlite",
    loss = ["sync-vs-async", "id-column-discovery"],
)]
pub fn stmt_insert<P: Params>(stmt: &mut Statement<'_>, params: P) -> Result<i64> {
    stmt.insert(params)
}

#[provekit::sugar(
    concept = "concept:sql-query",
    library = "rusqlite",
    loss = ["sync-vs-async", "cardinality-projected-to-boolean"],
)]
pub fn stmt_exists<P: Params>(stmt: &mut Statement<'_>, params: P) -> Result<bool> {
    stmt.exists(params)
}

// =============================================================================
// E. Transaction control
// =============================================================================

#[provekit::sugar(
    concept = "concept:sql-transaction-begin",
    library = "rusqlite",
    loss = ["sync-vs-async", "isolation-level", "nesting-depth-bound"],
)]
pub fn transaction<'conn>(conn: &'conn mut Connection) -> Result<Transaction<'conn>> {
    conn.transaction()
}

#[provekit::sugar(
    concept = "concept:sql-transaction-begin",
    library = "rusqlite",
    loss = ["sync-vs-async", "isolation-level", "nesting-depth-bound", "deferred-vs-immediate-vs-exclusive"],
)]
pub fn transaction_with_behavior<'conn>(
    conn: &'conn mut Connection,
    behavior: TransactionBehavior,
) -> Result<Transaction<'conn>> {
    conn.transaction_with_behavior(behavior)
}

#[provekit::sugar(
    concept = "concept:sql-transaction-begin",
    library = "rusqlite",
    loss = ["sync-vs-async", "isolation-level", "compile-time-nesting-check-bypass"],
)]
pub fn unchecked_transaction<'conn>(
    conn: &'conn Connection,
) -> Result<Transaction<'conn>> {
    conn.unchecked_transaction()
}

#[provekit::sugar(
    concept = "concept:sql-transaction-commit",
    library = "rusqlite",
    loss = ["sync-vs-async", "two-phase-commit-support"],
)]
pub fn tx_commit(tx: Transaction<'_>) -> Result<()> {
    tx.commit()
}

#[provekit::sugar(
    concept = "concept:sql-transaction-rollback",
    library = "rusqlite",
    loss = ["sync-vs-async", "partial-rollback-support"],
)]
pub fn tx_rollback(tx: Transaction<'_>) -> Result<()> {
    tx.rollback()
}

#[provekit::sugar(
    concept = "concept:sql-savepoint",
    library = "rusqlite",
    loss = ["nesting-depth-bound", "naming-discipline"],
)]
pub fn tx_savepoint<'tx>(tx: &'tx mut Transaction<'_>) -> Result<rusqlite::Savepoint<'tx>> {
    tx.savepoint()
}

#[provekit::sugar(
    concept = "concept:sql-transaction-rollback",
    library = "rusqlite",
    loss = ["runtime-policy-change"],
)]
pub fn tx_set_drop_behavior(
    tx: &mut Transaction<'_>,
    behavior: rusqlite::DropBehavior,
) {
    tx.set_drop_behavior(behavior)
}

// =============================================================================
// F. Row reading
// =============================================================================

#[provekit::sugar(
    concept = "concept:sql-row-get-column",
    library = "rusqlite",
    loss = ["null-handling", "type-coercion-mode"],
)]
pub fn row_get<I: rusqlite::RowIndex, T: rusqlite::types::FromSql>(
    row: &Row<'_>,
    idx: I,
) -> Result<T> {
    row.get(idx)
}

#[provekit::sugar(
    concept = "concept:sql-row-get-column",
    library = "rusqlite",
    loss = ["null-handling", "type-coercion-mode", "panic-on-failure"],
)]
pub fn row_get_unwrap<I: rusqlite::RowIndex, T: rusqlite::types::FromSql>(
    row: &Row<'_>,
    idx: I,
) -> T {
    row.get_unwrap(idx)
}

#[provekit::sugar(
    concept = "concept:sql-row-get-column",
    library = "rusqlite",
    loss = ["null-handling", "lifetime-bound-borrow"],
)]
pub fn row_get_ref<'row, I: rusqlite::RowIndex>(
    row: &'row Row<'row>,
    idx: I,
) -> Result<rusqlite::types::ValueRef<'row>> {
    row.get_ref(idx)
}

// =============================================================================
// G. Changes counting
// =============================================================================

#[provekit::sugar(
    concept = "concept:insert-and-get-id",
    library = "rusqlite",
    loss = ["per-connection-not-per-statement", "rowid-vs-integer-pk"],
)]
pub fn last_insert_rowid(conn: &Connection) -> i64 {
    conn.last_insert_rowid()
}

#[provekit::sugar(
    concept = "concept:sql-changes-count",
    library = "rusqlite",
    loss = ["per-statement-vs-cumulative", "transaction-scope"],
)]
pub fn changes(conn: &Connection) -> u64 {
    conn.changes()
}

#[provekit::sugar(
    concept = "concept:sql-changes-count",
    library = "rusqlite",
    loss = ["cumulative-since-connection-open", "transaction-scope"],
)]
pub fn total_changes(conn: &Connection) -> u64 {
    conn.total_changes()
}

// =============================================================================
// H. Connection state observation
// =============================================================================

#[provekit::sugar(
    concept = "concept:contract-observation",
    library = "rusqlite",
    observed_dimension = "autocommit-mode",
)]
pub fn is_autocommit(conn: &Connection) -> bool {
    conn.is_autocommit()
}

#[provekit::sugar(
    concept = "concept:contract-observation",
    library = "rusqlite",
    observed_dimension = "pending-statement-presence",
)]
pub fn is_busy(conn: &Connection) -> bool {
    conn.is_busy()
}

#[provekit::sugar(
    concept = "concept:contract-observation",
    library = "rusqlite",
    observed_dimension = "write-permission",
)]
pub fn is_readonly<N: rusqlite::Name>(conn: &Connection, db_name: N) -> Result<bool> {
    conn.is_readonly(db_name)
}

#[provekit::sugar(
    concept = "concept:contract-observation",
    library = "rusqlite",
    observed_dimension = "cache-state",
    loss = ["flush-side-effect"],
)]
pub fn cache_flush(conn: &Connection) -> Result<()> {
    conn.cache_flush()
}

// =============================================================================
// I. Statement metadata observation
// =============================================================================

#[provekit::sugar(
    concept = "concept:contract-observation",
    library = "rusqlite",
    observed_dimension = "column-names",
)]
pub fn stmt_column_names<'stmt>(stmt: &'stmt Statement<'_>) -> Vec<&'stmt str> {
    stmt.column_names()
}

#[provekit::sugar(
    concept = "concept:contract-observation",
    library = "rusqlite",
    observed_dimension = "column-count",
)]
pub fn stmt_column_count(stmt: &Statement<'_>) -> usize {
    stmt.column_count()
}

#[provekit::sugar(
    concept = "concept:contract-observation",
    library = "rusqlite",
    observed_dimension = "column-name-at-index",
)]
pub fn stmt_column_name<'stmt>(stmt: &'stmt Statement<'_>, col: usize) -> Result<&'stmt str> {
    stmt.column_name(col)
}

#[provekit::sugar(
    concept = "concept:contract-observation",
    library = "rusqlite",
    observed_dimension = "column-index-of-name",
)]
pub fn stmt_column_index(stmt: &Statement<'_>, name: &str) -> Result<usize> {
    stmt.column_index(name)
}

#[provekit::sugar(
    concept = "concept:contract-observation",
    library = "rusqlite",
    observed_dimension = "expanded-sql-text",
)]
pub fn stmt_expanded_sql(stmt: &Statement<'_>) -> Option<String> {
    stmt.expanded_sql()
}

#[provekit::sugar(
    concept = "concept:contract-observation",
    library = "rusqlite",
    observed_dimension = "parameter-count",
)]
pub fn stmt_parameter_count(stmt: &Statement<'_>) -> usize {
    stmt.parameter_count()
}

#[provekit::sugar(
    concept = "concept:contract-observation",
    library = "rusqlite",
    observed_dimension = "parameter-name-at-index",
)]
pub fn stmt_parameter_name<'stmt>(
    stmt: &'stmt Statement<'_>,
    idx: usize,
) -> Option<&'stmt str> {
    stmt.parameter_name(idx)
}

#[provekit::sugar(
    concept = "concept:contract-observation",
    library = "rusqlite",
    observed_dimension = "parameter-index-of-name",
)]
pub fn stmt_parameter_index(stmt: &Statement<'_>, name: &str) -> Result<Option<usize>> {
    stmt.parameter_index(name)
}

// =============================================================================
// J. Concurrency control
// =============================================================================

#[provekit::sugar(
    concept = "concept:sql-busy-timeout",
    library = "rusqlite",
    loss = ["sync-vs-async", "callback-vs-timeout-shape"],
)]
pub fn busy_timeout(conn: &Connection, timeout: Duration) -> Result<()> {
    conn.busy_timeout(timeout)
}

// =============================================================================
// Refusals
// =============================================================================
//
// Each refusal is a signed signpost. The substrate publishes the demand the
// shim declines to fill, naming the cluster constraint that would close it.
// The lift kit emits a `refusal-memento` IR record per attribute; cmd_mint
// signs each as a `RefusalMemento` envelope member.

#[provekit::refuse(
    surface = "rusqlite::Connection::backup",
    concept = "concept:sql-physical-backup",
    reason = "SQLite-binary-specific physical backup. Postgres has pg_basebackup (out-of-band, not a connection method); MySQL has equivalent. N=1 across connection-level APIs for now; cluster does not yet form. Refusing rather than minting a single-kit concept hub.",
    would_close_with_cluster = "Connection-level physical-backup method on >=2 SQL drivers",
)]
pub mod refused_backup {}

#[provekit::refuse(
    surface = "rusqlite::Connection::blob_open",
    concept = "concept:sql-blob-handle",
    reason = "Incremental BLOB I/O is SQLite-specific surface shape. Postgres has lo_open with a different lifecycle model; MySQL has no equivalent. The semantic shapes diverge enough that a single-kit cluster would not serve cross-library composition.",
    would_close_with_cluster = "Incremental BLOB I/O on >=2 SQL drivers with structurally compatible handle semantics",
)]
pub mod refused_blob_open {}

#[provekit::refuse(
    surface = "rusqlite::Connection::load_extension",
    concept = "concept:dynamic-library-load",
    reason = "OS-level dynamic-library-load, not SQL-domain. The right concept lives at the OS-binding tier (Rust libloading, Python ctypes, C dlopen, etc.) not at the SQL kit tier. Refusing rather than crossing tier boundaries.",
    would_close_with_cluster = "OS-tier kit minting (separate from SQL-driver-tier)",
)]
pub mod refused_load_extension {}

#[provekit::refuse(
    surface = "rusqlite::Connection::create_collation",
    concept = "concept:sql-collation-register",
    reason = "Custom string-comparison callback registration. Postgres supports custom collations via CREATE COLLATION SQL; SQLite registers them as host-language closures. The mechanism diverges enough that a single-kit binding would carry opaque callback semantics.",
    would_close_with_cluster = "Custom collation registration on >=2 SQL drivers with structurally compatible callback semantics",
)]
pub mod refused_create_collation {}

#[provekit::refuse(
    surface = "rusqlite::Connection::busy_handler",
    concept = "concept:sql-busy-handler",
    reason = "Dynamic callback-based busy handler. concept:sql-busy-timeout (which this kit DOES bind) covers the timeout-shaped variant; the callback-shaped variant is SQLite-specific and Postgres/MySQL have no analog at this granularity.",
    would_close_with_cluster = "Callback-based busy-collision handling on >=2 SQL drivers",
)]
pub mod refused_busy_handler {}

#[provekit::refuse(
    surface = "rusqlite::Row::get_pointer",
    concept = "concept:sql-row-pointer-type",
    reason = "Pointer-passing through SQLite's auxiliary data interface. SQLite-specific feature; not a concept the substrate carries today.",
    would_close_with_cluster = "Pointer-passing row column type on >=2 SQL drivers",
)]
pub mod refused_row_get_pointer {}

#[provekit::refuse(
    surface = "rusqlite::Connection::pragma_query",
    concept = "concept:sql-pragma",
    reason = "PRAGMA bindings deferred to provekit-shim-rusqlite v0.2. rusqlite 0.39's PRAGMA API uses `DatabaseName<'_>` whose exact crate-level re-export path needs cargo-verified confirmation before binding. Substrate-honest discipline: refuse to ship API shapes that have not been verified to compile against the upstream crate.",
    would_close_with_cluster = "Cargo-verified API shape for rusqlite 0.39 PRAGMA family",
)]
pub mod refused_pragma_query {}

#[provekit::refuse(
    surface = "rusqlite::Connection::pragma_query_value",
    concept = "concept:sql-pragma",
    reason = "Same as refused_pragma_query: API shape verification pending for v0.2.",
    would_close_with_cluster = "Cargo-verified API shape for rusqlite 0.39 PRAGMA family",
)]
pub mod refused_pragma_query_value {}

#[provekit::refuse(
    surface = "rusqlite::Connection::pragma_update",
    concept = "concept:sql-pragma",
    reason = "Same as refused_pragma_query: API shape verification pending for v0.2.",
    would_close_with_cluster = "Cargo-verified API shape for rusqlite 0.39 PRAGMA family",
)]
pub mod refused_pragma_update {}

#[provekit::refuse(
    surface = "rusqlite::Connection::db_name",
    concept = "concept:contract-observation",
    reason = "Feature-gated behind `modern_sqlite` and return type signature (Result<String> vs Option<String>) needs cargo-verified confirmation. Deferred to v0.2.",
    would_close_with_cluster = "Cargo-verified API signature for db_name under modern_sqlite feature",
)]
pub mod refused_db_name {}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_in_memory_roundtrip() {
        let conn = open_in_memory().expect("open in-memory db");
        execute(
            &conn,
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)",
            [],
        )
        .expect("create table");
        let inserted = execute(
            &conn,
            "INSERT INTO users (name) VALUES (?1)",
            rusqlite::params!["alice"],
        )
        .expect("insert");
        assert_eq!(inserted, 1);
        let id = last_insert_rowid(&conn);
        assert_eq!(id, 1);
        let name: String = query_row(
            &conn,
            "SELECT name FROM users WHERE id = ?1",
            rusqlite::params![id],
            |row| row_get(row, 0),
        )
        .expect("query row");
        assert_eq!(name, "alice");
    }

    #[test]
    fn prepared_statement_query_map() {
        let conn = open_in_memory().expect("open in-memory db");
        execute(
            &conn,
            "CREATE TABLE animals (kind TEXT NOT NULL, count INTEGER NOT NULL)",
            [],
        )
        .expect("create table");
        execute_batch(
            &conn,
            "INSERT INTO animals VALUES ('cat', 2);
             INSERT INTO animals VALUES ('dog', 1);
             INSERT INTO animals VALUES ('owl', 3);",
        )
        .expect("seed");
        let mut stmt = prepare(&conn, "SELECT kind, count FROM animals ORDER BY count")
            .expect("prepare");
        let rows: Vec<(String, i64)> = stmt_query_map(&mut stmt, [], |row| {
            Ok((row_get(row, 0)?, row_get(row, 1)?))
        })
        .expect("query_map")
        .collect::<Result<Vec<_>>>()
        .expect("collect");
        assert_eq!(
            rows,
            vec![
                ("dog".to_string(), 1),
                ("cat".to_string(), 2),
                ("owl".to_string(), 3),
            ]
        );
    }

    #[test]
    fn transaction_commit_and_rollback() {
        let mut conn = open_in_memory().expect("open in-memory db");
        execute(
            &conn,
            "CREATE TABLE counters (name TEXT PRIMARY KEY, value INTEGER NOT NULL)",
            [],
        )
        .expect("create table");

        let tx = transaction(&mut conn).expect("begin tx");
        execute(&tx, "INSERT INTO counters VALUES ('a', 1)", []).expect("insert in tx");
        tx_commit(tx).expect("commit");

        let count: i64 = query_row(&conn, "SELECT value FROM counters WHERE name = 'a'", [], |row| {
            row_get(row, 0)
        })
        .expect("query");
        assert_eq!(count, 1);

        let tx = transaction(&mut conn).expect("begin tx 2");
        execute(&tx, "INSERT INTO counters VALUES ('b', 2)", []).expect("insert in tx 2");
        tx_rollback(tx).expect("rollback");

        let exists: bool = query_row(
            &conn,
            "SELECT EXISTS(SELECT 1 FROM counters WHERE name = 'b')",
            [],
            |row| row_get(row, 0),
        )
        .expect("query exists");
        assert!(!exists);
    }
}
