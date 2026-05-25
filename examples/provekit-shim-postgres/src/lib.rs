// SPDX-License-Identifier: Apache-2.0
//
// provekit-shim-postgres: substrate-honest concept bindings for the
// `postgres` sync Postgres driver.
//
// Sister shim to provekit-shim-rusqlite — same concept hub (concept:sql-*),
// different database engine. Demonstrates cross-engine federation: any
// downstream consumer can route through these same concept CIDs to either
// the rusqlite realization (SQLite) or the postgres realization (Postgres).
// The substrate composes both into one M+N hub.
//
// Each binding carries its `loss` array (the dimensions where postgres's
// realization is bounded-lossy against the concept's structural
// admissibility) and, for refusals, the surfaces this kit declines to
// bind in v0.1.

pub use postgres::{Client, Config, NoTls, Row, Statement, Transaction};

// =============================================================================
// A. Connection lifecycle
// =============================================================================

#[provekit::sugar(
    concept = "concept:sql-connection-open",
    library = "postgres",
    family = "concept:family:sql",
    version = "0.19",
    loss = ["sync-vs-async", "tls-bring-your-own", "connection-pooling"],
)]
pub fn connect(params: &str) -> Result<Client, postgres::Error> {
    Client::connect(params, NoTls)
}

#[provekit::sugar(
    concept = "concept:sql-connection-close",
    library = "postgres",
    family = "concept:family:sql",
    version = "0.19",
    loss = ["sync-vs-async", "drop-vs-explicit-close"],
)]
pub fn close(client: Client) -> Result<(), postgres::Error> {
    client.close()
}

// =============================================================================
// B. Query execution at the Client level
// =============================================================================

#[provekit::sugar(
    concept = "concept:sql-execute",
    library = "postgres",
    family = "concept:family:sql",
    version = "0.19",
    loss = ["sync-vs-async", "transaction-isolation", "row-count-semantics"],
)]
pub fn execute(
    client: &mut Client,
    sql: &str,
    params: &[&(dyn postgres::types::ToSql + Sync)],
) -> Result<u64, postgres::Error> {
    client.execute(sql, params)
}

#[provekit::sugar(
    concept = "concept:sql-query-all",
    library = "postgres",
    family = "concept:family:sql",
    version = "0.19",
    loss = ["sync-vs-async", "row-cardinality", "type-coercion"],
)]
pub fn query(
    client: &mut Client,
    sql: &str,
    params: &[&(dyn postgres::types::ToSql + Sync)],
) -> Result<Vec<Row>, postgres::Error> {
    client.query(sql, params)
}

#[provekit::sugar(
    concept = "concept:sql-query-row",
    library = "postgres",
    family = "concept:family:sql",
    version = "0.19",
    loss = ["sync-vs-async", "row-cardinality-exactly-one", "type-coercion"],
)]
pub fn query_one(
    client: &mut Client,
    sql: &str,
    params: &[&(dyn postgres::types::ToSql + Sync)],
) -> Result<Row, postgres::Error> {
    client.query_one(sql, params)
}

// =============================================================================
// C. Prepared statements
// =============================================================================

#[provekit::sugar(
    concept = "concept:sql-prepare",
    library = "postgres",
    family = "concept:family:sql",
    version = "0.19",
    loss = ["sync-vs-async", "named-statement-cache"],
)]
pub fn prepare(client: &mut Client, sql: &str) -> Result<Statement, postgres::Error> {
    client.prepare(sql)
}

// =============================================================================
// D. Transactions
// =============================================================================

#[provekit::sugar(
    concept = "concept:sql-transaction-begin",
    library = "postgres",
    family = "concept:family:sql",
    version = "0.19",
    loss = ["sync-vs-async", "isolation-level-default"],
)]
pub fn transaction(client: &mut Client) -> Result<Transaction, postgres::Error> {
    client.transaction()
}

#[provekit::sugar(
    concept = "concept:sql-transaction-commit",
    library = "postgres",
    family = "concept:family:sql",
    version = "0.19",
    loss = ["sync-vs-async"],
)]
pub fn commit(tx: Transaction) -> Result<(), postgres::Error> {
    tx.commit()
}

#[provekit::sugar(
    concept = "concept:sql-transaction-rollback",
    library = "postgres",
    family = "concept:family:sql",
    version = "0.19",
    loss = ["sync-vs-async"],
)]
pub fn rollback(tx: Transaction) -> Result<(), postgres::Error> {
    tx.rollback()
}

// =============================================================================
// E. Refusals — surfaces declined in v0.1, signposted for the concept hub.
// =============================================================================

#[provekit::refuse(
    surface = "postgres::Client::copy_in",
    concept = "concept:sql-bulk-load",
    reason = "streaming COPY protocol has no concept-hub binding yet; would close once concept:streaming-ingest is minted with byte-stream effect signature",
    would_close_with_cluster = "streaming-ingest-with-typed-rows",
)]
mod _refuse_copy_in {}

#[provekit::refuse(
    surface = "postgres::Client::copy_out",
    concept = "concept:sql-bulk-export",
    reason = "streaming COPY OUT mirror of copy_in; same concept gap",
    would_close_with_cluster = "streaming-ingest-with-typed-rows",
)]
mod _refuse_copy_out {}

#[provekit::refuse(
    surface = "postgres::Notifications",
    concept = "concept:sql-pub-sub",
    reason = "Postgres LISTEN/NOTIFY has no SQL-concept-hub equivalent; would close with concept:async-channel-of-named-events",
    would_close_with_cluster = "async-channel-of-named-events",
)]
mod _refuse_notifications {}
