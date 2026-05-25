// SPDX-License-Identifier: Apache-2.0
//
// @provekit-shim/typescript-pg: substrate-honest concept bindings for the
// asynchronous node-`pg` (PostgreSQL) driver.
//
// Paper 24 section 3 ("After Vendoring"): libraries ship their own sugar.
// This shim is the TypeScript analogue of provekit-shim-postgres (the Rust
// `postgres` sync driver) and the sister to provekit-shim-better-sqlite3.
// Same concept hub (concept:sql-*), different database engine and async shape.
// The TypeScript lifter reads this file with `layer = "library-bindings"`,
// emits `library-sugar-binding-entry` and `refusal-memento` IR, and the shared
// `provekit mint --library-bindings` path packages those entries into the
// package-root `provekit.proof` envelope. The realize kit
// (provekit-realize-typescript-pg) resolves THIS package's `provekit.proof`
// from its own node_modules at runtime; there is no central JSON registry.
//
// CARDINALITY SPLIT (#1468): node-`pg`'s sole read primitive is
// `client.query(sql, params)` returning `{ rows, rowCount }`. The cardinality
// is selected at the binding site by post-condition projection:
//   * queryAll  -> result.rows                 -> concept:sql-query-all  (materialized array)
//   * queryRow  -> result.rows[0] (or undefined)-> concept:sql-query-row  (at most one row)
// node-`pg` base has no first-class lazy cursor (that is the separate
// `pg-cursor` package), so concept:sql-query-iterate is left UNBOUND, mirroring
// the rust `postgres` shim which binds no -iterate either.
//
// Migrate-path arity-2 bindings (#1468): the better-sqlite3 -> typescript-pg
// migrate probe realizes at the fixed 2-param (sql, args) shape (params
// ["sql","args"], paramTypes ["string","unknown[]"]). The connection-as-receiver
// query bindings are arity-3 (the wrong shape for the migrate probe), so a
// migrate sibling per probed concept is minted with a free `pool` binding the
// migrate assembler hoists (not a method receiver) and `sql`/`args` mapping to
// ${param0}/${param1}. Mirrors provekit-shim-python-aiosqlite's migrate_* trio.
//
// Async honesty (mirrors the aiosqlite shim, NOT the sync rust `postgres`
// shim): node-`pg` IS the async driver, so `sync-vs-async` is NOT a loss
// dimension on any binding here. The functions are `async`; the lifter records
// the async body shape; the realizer emits `async function`. Where the sync
// rust `postgres` shim declares `sync-vs-async`, this async kit drops it and
// keeps only the genuinely-lost dimensions.

import { Client, Pool, PoolClient, QueryResult } from "pg";
import { sugar } from "provekit";

export type Queryable = Client | Pool | PoolClient;
export type Params = readonly unknown[];

// ============================================================
// A. Connection management
// ============================================================

@sugar.bind({
  concept: "concept:sql-connection-open",
  library: "pg",
  family: "concept:family:sql",
  version: "8.16.3",
  loss: ["tls-bring-your-own"],
})
export async function connect(connectionString: string): Promise<Client> {
  const client = new Client({ connectionString });
  await client.connect();
  return client;
}

@sugar.bind({
  concept: "concept:sql-connection-open",
  library: "pg",
  family: "concept:family:sql",
  version: "8.16.3",
  loss: ["tls-bring-your-own", "pool-vs-single-connection"],
})
export function openPool(connectionString: string): Pool {
  return new Pool({ connectionString });
}

@sugar.bind({
  concept: "concept:sql-connection-close",
  library: "pg",
  family: "concept:family:sql",
  version: "8.16.3",
})
export async function close(client: Client): Promise<void> {
  await client.end();
}

@sugar.bind({
  concept: "concept:sql-connection-close",
  library: "pg",
  family: "concept:family:sql",
  version: "8.16.3",
  loss: ["pool-drain-semantics"],
})
export async function closePool(pool: Pool): Promise<void> {
  await pool.end();
}

// ============================================================
// B. Query execution at the connection level
// ============================================================

@sugar.bind({
  concept: "concept:sql-execute",
  library: "pg",
  family: "concept:family:sql",
  version: "8.16.3",
  loss: ["row-count-semantics"],
})
export async function execute(
  pool: Queryable,
  sql: string,
  params: Params = [],
): Promise<{ rows_affected: number; last_insert_id: unknown }> {
  const result = await pool.query(sql + " RETURNING id", params as unknown[]);
  return { rows_affected: result.rowCount ?? 0, last_insert_id: result.rows[0]?.id ?? null };
}

@sugar.bind({
  concept: "concept:sql-query-all",
  library: "pg",
  family: "concept:family:sql",
  version: "8.16.3",
  loss: ["type-coercion"],
})
export async function queryAll(pool: Queryable, sql: string, params: Params = []): Promise<unknown[]> {
  const result = await pool.query(sql, params as unknown[]);
  return result.rows;
}

@sugar.bind({
  concept: "concept:sql-query-row",
  library: "pg",
  family: "concept:family:sql",
  version: "8.16.3",
  loss: ["type-coercion", "row-cardinality-at-most-one"],
})
export async function queryRow(pool: Queryable, sql: string, params: Params = []): Promise<unknown> {
  const result = await pool.query(sql, params as unknown[]);
  return result.rows[0];
}

@sugar.bind({
  concept: "concept:insert-and-get-id",
  library: "pg",
  family: "concept:family:sql",
  version: "8.16.3",
  loss: ["returning-clause-vs-last-insert-id"],
})
export async function insertAndGetId(pool: Queryable, sql: string, params: Params = []): Promise<number> {
  const result = await pool.query<{ id: number }>(sql + " RETURNING id", params as unknown[]);
  return Number(result.rows[0]?.id ?? 0);
}

// ============================================================
// C. Prepared / named statements
// ============================================================

@sugar.bind({
  concept: "concept:sql-prepare",
  library: "pg",
  family: "concept:family:sql",
  version: "8.16.3",
  loss: ["named-statement-cache", "prepare-is-implicit-on-first-execute"],
})
export async function prepare(pool: Queryable, name: string, sql: string, params: Params = []): Promise<QueryResult> {
  // node-pg has no standalone prepare(); a named query is prepared implicitly
  // on first execution. The closest binding is a named parameterized query.
  return pool.query({ name, text: sql, values: params as unknown[] });
}

// ============================================================
// D. Transactions
//    node-pg expresses transactions as raw SQL statements over a single
//    client/connection (BEGIN / COMMIT / ROLLBACK), mirroring the postgres
//    wire protocol. There is no transaction-object handle as in rust postgres.
// ============================================================

@sugar.bind({
  concept: "concept:sql-transaction-begin",
  library: "pg",
  family: "concept:family:sql",
  version: "8.16.3",
  loss: ["isolation-level-default", "no-transaction-handle"],
})
export async function transactionBegin(pool: Queryable): Promise<void> {
  await pool.query("BEGIN");
}

@sugar.bind({
  concept: "concept:sql-transaction-commit",
  library: "pg",
  family: "concept:family:sql",
  version: "8.16.3",
  loss: ["no-transaction-handle"],
})
export async function transactionCommit(pool: Queryable): Promise<void> {
  await pool.query("COMMIT");
}

@sugar.bind({
  concept: "concept:sql-transaction-rollback",
  library: "pg",
  family: "concept:family:sql",
  version: "8.16.3",
  loss: ["no-transaction-handle"],
})
export async function transactionRollback(pool: Queryable): Promise<void> {
  await pool.query("ROLLBACK");
}

// ============================================================
// E. Migrate-shaped 2-param SQL bindings (#1468)
//    Each free `pool` name passes through the param->placeholder projection
//    unchanged; only `sql`/`args` map to ${param0}/${param1}. The migrate
//    assembler hoists `pool` into scope (the typescript-pg variable convention,
//    matching the receiver-binding param name and the flat JSON this shim
//    replaces). Mirrors the python-aiosqlite migrate_* trio with async pg
//    templates.
// ============================================================

@sugar.bind({
  concept: "concept:sql-execute",
  library: "pg",
  family: "concept:family:sql",
  version: "8.16.3",
  loss: ["row-count-semantics"],
})
export async function migrateExecute(sql: string, args: unknown[]): Promise<{ rows_affected: number; last_insert_id: unknown }> {
  const result = await pool.query(sql + " RETURNING id", args);
  return { rows_affected: result.rowCount ?? 0, last_insert_id: result.rows[0]?.id ?? null };
}

@sugar.bind({
  concept: "concept:insert-and-get-id",
  library: "pg",
  family: "concept:family:sql",
  version: "8.16.3",
  loss: ["returning-clause-vs-last-insert-id"],
})
export async function migrateInsertAndGetId(sql: string, args: unknown[]): Promise<number> {
  const result = await pool.query<{ id: number }>(sql + " RETURNING id", args);
  return Number(result.rows[0]?.id ?? 0);
}

@sugar.bind({
  concept: "concept:sql-query-all",
  library: "pg",
  family: "concept:family:sql",
  version: "8.16.3",
  loss: ["type-coercion"],
})
export async function migrateQuery(sql: string, args: unknown[]): Promise<unknown[]> {
  const result = await pool.query(sql, args);
  return result.rows;
}

@sugar.bind({
  concept: "concept:sql-query-row",
  library: "pg",
  family: "concept:family:sql",
  version: "8.16.3",
  loss: ["type-coercion", "row-cardinality-at-most-one"],
})
export async function migrateQueryRow(sql: string, args: unknown[]): Promise<unknown> {
  const result = await pool.query(sql, args);
  return result.rows[0];
}

// ============================================================
// F. Refusals — surfaces declined in v0.1, signposted for the concept hub.
// ============================================================

@sugar.refuse({
  surface: "typescript-bind",
  concept: "concept:sql-query-iterate",
  reason: "node-pg base has no first-class lazy cursor; row streaming requires the separate pg-cursor / pg-query-stream package. The lazy single-pass cardinality has no in-driver surface, mirroring the rust postgres shim which binds no -iterate either.",
  would_close_with_cluster: "lazy single-pass cursor on the node-pg base driver",
})
class RefusedQueryIterate {}

@sugar.refuse({
  surface: "typescript-bind",
  concept: "concept:sql-bulk-load",
  reason: "streaming COPY FROM has no concept-hub binding yet and node-pg exposes it only via the separate pg-copy-streams package; would close once concept:streaming-ingest is minted with a byte-stream effect signature",
  would_close_with_cluster: "streaming-ingest-with-typed-rows",
})
class RefusedCopyIn {}

@sugar.refuse({
  surface: "typescript-bind",
  concept: "concept:sql-bulk-export",
  reason: "streaming COPY TO mirror of copy_in; same concept gap, also requires pg-copy-streams",
  would_close_with_cluster: "streaming-ingest-with-typed-rows",
})
class RefusedCopyOut {}

@sugar.refuse({
  surface: "typescript-bind",
  concept: "concept:sql-pub-sub",
  reason: "Postgres LISTEN/NOTIFY surfaces as client 'notification' events with no SQL-concept-hub equivalent; would close with concept:async-channel-of-named-events",
  would_close_with_cluster: "async-channel-of-named-events",
})
class RefusedNotifications {}
