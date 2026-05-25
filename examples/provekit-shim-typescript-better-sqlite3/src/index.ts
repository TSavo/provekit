// SPDX-License-Identifier: Apache-2.0
//
// provekit-shim-better-sqlite3: substrate-honest concept bindings for the
// synchronous better-sqlite3 Node driver.
//
// Paper 24 section 3 ("After Vendoring"): libraries ship their own sugar.
// This shim is the TypeScript analogue of provekit-shim-rusqlite. The
// TypeScript lifter reads this file with `layer = "library-bindings"`,
// emits `library-sugar-binding-entry` and `refusal-memento` IR, and the
// shared `provekit mint --library-bindings` path packages those entries
// into the package-root `provekit.proof` envelope.
//
// MULTIPLE SUGARS PER CONCEPT
// ---------------------------
// Sugar format is taste; a concept can carry multiple bindings. For each
// connection-level concept this shim ships BOTH alternative surfaces:
//
//   * receiver-as-param (arity-3): `queryAll(db, sql, params)` — the db is
//     an explicit operand. Body: `db.prepare(sql).all(params)`.
//   * receiver-free (arity-2): `queryAllFree(sql, args)` — the db is a FREE
//     name in scope (ambient `declare const db`), only the genuine operands
//     are parameters. Body: `db.prepare(sql).all(args)`.
//
// Both realize the same contract. The materialize matcher selects by the
// consumer carrier's shape: a `(sql, args)` carrier matches the arity-2
// receiver-free binding; a `(db, sql, params)` carrier matches the arity-3
// one. The arity barrier is the disambiguator.
//
// Prepared-statement-level operations are a DISTINCT concept
// (`concept:sql-stmt-*`): they act on an already-prepared Statement, not on
// the connection, so they never collide with the connection-level arity-2
// bindings.
//
// The lifter rewrites parameter names to `${param0}`/`${param1}`/... but
// leaves free receiver names (`db`, `stmt`, `result`) literal. This file is
// a SPEC, never executed; the `declare const` receivers are ambient.

import Database from "better-sqlite3";
import { sugar } from "provekit";

export type Params = readonly unknown[] | Record<string, unknown>;

// Ambient receivers: free names the receiver-free bodies reference. Never
// executed; ambient declarations typecheck without a runtime value.
declare const db: Database.Database;
declare const stmt: Database.Statement;
declare const result: Database.RunResult;

// ============================================================
// A. Connection management
// ============================================================

@sugar.bind({
  concept: "concept:sql-connection-open",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
})
export function open(filename: string): Database.Database {
  return new Database(filename);
}

@sugar.bind({
  concept: "concept:sql-connection-open",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
})
export function openInMemory(): Database.Database {
  return new Database(":memory:");
}

@sugar.bind({
  concept: "concept:sql-connection-open",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
  loss: ["readonly-flag", "fileMustExist-flag"],
})
export function openWithOptions(filename: string, options: Database.Options): Database.Database {
  return new Database(filename, options);
}

@sugar.bind({
  concept: "concept:sql-connection-close",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
})
export function close(db: Database.Database): void {
  db.close();
}

// ============================================================
// B. Statement execution at connection level
//    Each concept ships arity-3 (receiver-as-param) + arity-2 (receiver-free).
// ============================================================

@sugar.bind({
  concept: "concept:sql-execute",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
})
export function execute(db: Database.Database, sql: string, params: Params = []): Database.RunResult {
  return db.prepare(sql).run(params);
}

@sugar.bind({
  concept: "concept:sql-execute",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
})
export function executeFree(sql: string, args: Params = []): Database.RunResult {
  return db.prepare(sql).run(args);
}

@sugar.bind({
  concept: "concept:sql-execute",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
  loss: ["multi-statement-support"],
})
export function executeBatch(db: Database.Database, sql: string): void {
  db.exec(sql);
}

@sugar.bind({
  concept: "concept:sql-query",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
})
export function queryRow(db: Database.Database, sql: string, params: Params = []): unknown {
  return db.prepare(sql).get(params);
}

@sugar.bind({
  concept: "concept:sql-query",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
})
export function queryAll(db: Database.Database, sql: string, params: Params = []): unknown[] {
  return db.prepare(sql).all(params);
}

@sugar.bind({
  concept: "concept:sql-query",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
})
export function queryAllFree(sql: string, args: Params = []): unknown[] {
  return db.prepare(sql).all(args);
}

@sugar.bind({
  concept: "concept:sql-query",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
})
export function queryRowFree(sql: string, args: Params = []): unknown {
  return db.prepare(sql).get(args);
}

// ============================================================
// C. Statement preparation
// ============================================================

@sugar.bind({
  concept: "concept:sql-prepare",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
})
export function prepare(db: Database.Database, sql: string): Database.Statement {
  return db.prepare(sql);
}

@sugar.bind({
  concept: "concept:sql-prepare",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
  loss: ["bind-returns-same-stmt"],
})
export function stmtBind(stmt: Database.Statement, ...args: unknown[]): Database.Statement {
  return stmt.bind(...args);
}

// ============================================================
// D. Statement execution (prepared-statement level).
//    DISTINCT concept (concept:sql-stmt-*): operates on a prepared
//    Statement, not on the connection. No arity-2 collision with the
//    connection-level concepts above.
// ============================================================

@sugar.bind({
  concept: "concept:sql-stmt-execute",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
})
export function stmtRun(stmt: Database.Statement, params: Params = []): Database.RunResult {
  return stmt.run(params);
}

@sugar.bind({
  concept: "concept:sql-stmt-query",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
})
export function stmtAll(stmt: Database.Statement, params: Params = []): unknown[] {
  return stmt.all(params);
}

@sugar.bind({
  concept: "concept:sql-stmt-query",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
})
export function stmtGet(stmt: Database.Statement, params: Params = []): unknown {
  return stmt.get(params);
}

@sugar.bind({
  concept: "concept:sql-stmt-query",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
  loss: ["generator-protocol"],
})
export function stmtIterate(stmt: Database.Statement, params: Params = []): IterableIterator<unknown> {
  return stmt.iterate(params) as IterableIterator<unknown>;
}

// ============================================================
// E. Transactions
// ============================================================

@sugar.bind({
  concept: "concept:sql-transaction-begin",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
})
export function transaction<T>(db: Database.Database, body: () => T): T {
  return db.transaction(body)();
}

@sugar.bind({
  concept: "concept:sql-transaction-begin",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
  loss: ["deferred-isolation-level"],
})
export function transactionDeferred<T>(db: Database.Database, body: () => T): T {
  return db.transaction(body).deferred();
}

@sugar.bind({
  concept: "concept:sql-transaction-begin",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
  loss: ["immediate-isolation-level"],
})
export function transactionImmediate<T>(db: Database.Database, body: () => T): T {
  return db.transaction(body).immediate();
}

@sugar.bind({
  concept: "concept:sql-transaction-begin",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
  loss: ["exclusive-isolation-level"],
})
export function transactionExclusive<T>(db: Database.Database, body: () => T): T {
  return db.transaction(body).exclusive();
}

// ============================================================
// F. Row reading modes (prepared-statement level)
// ============================================================

@sugar.bind({
  concept: "concept:sql-stmt-query",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
  loss: ["named-column-access"],
})
export function rowByIndex(stmt: Database.Statement, params: Params = []): unknown[] {
  return (stmt.raw(true).get(params) ?? []) as unknown[];
}

@sugar.bind({
  concept: "concept:sql-stmt-query",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
})
export function rowAsObject(stmt: Database.Statement, params: Params = []): Record<string, unknown> {
  return (stmt.raw(false).get(params) ?? {}) as Record<string, unknown>;
}

// ============================================================
// G. Mutation result observation
// ============================================================

@sugar.bind({
  concept: "concept:insert-and-get-id",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
  observed_dimension: "last-insert-rowid",
})
export function lastInsertRowid(result: Database.RunResult): number | bigint {
  return result.lastInsertRowid;
}

@sugar.bind({
  concept: "concept:sql-changes-affected",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
  observed_dimension: "row-count",
})
export function changes(result: Database.RunResult): number {
  return result.changes;
}

// ============================================================
// H. Connection state observation
// ============================================================

@sugar.bind({
  concept: "concept:sql-connection-state",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
  observed_dimension: "in-transaction",
})
export function isInTransaction(db: Database.Database): boolean {
  return db.inTransaction;
}

@sugar.bind({
  concept: "concept:sql-connection-state",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
  observed_dimension: "open",
})
export function isOpen(db: Database.Database): boolean {
  return db.open;
}

@sugar.bind({
  concept: "concept:sql-connection-state",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
  observed_dimension: "readonly",
})
export function isReadonly(db: Database.Database): boolean {
  return db.readonly;
}

@sugar.bind({
  concept: "concept:sql-connection-state",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
  observed_dimension: "memory",
})
export function isMemory(db: Database.Database): boolean {
  return db.memory;
}

@sugar.bind({
  concept: "concept:sql-connection-state",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
  observed_dimension: "filename",
})
export function dbName(db: Database.Database): string {
  return db.name;
}

// ============================================================
// I. Statement metadata (prepared-statement level)
// ============================================================

@sugar.bind({
  concept: "concept:sql-stmt-columns",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
})
export function stmtColumns(stmt: Database.Statement): Database.ColumnDefinition[] {
  return stmt.columns();
}

@sugar.bind({
  concept: "concept:sql-stmt-source",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
})
export function stmtSource(stmt: Database.Statement): string {
  return stmt.source;
}

@sugar.bind({
  concept: "concept:sql-stmt-reader",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
  observed_dimension: "reader",
})
export function stmtReader(stmt: Database.Statement): boolean {
  return stmt.reader;
}

// ============================================================
// J. Concurrency / timeout
// ============================================================

@sugar.bind({
  concept: "concept:sql-busy-timeout",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
})
export function busyTimeout(db: Database.Database, ms: number): Database.Database {
  return db.pragma(`busy_timeout = ${ms}`) as unknown as Database.Database;
}

@sugar.bind({
  concept: "concept:sql-pragma",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
  loss: ["pragma-value-type-varies"],
})
export function pragmaQuery(db: Database.Database, pragma: string): unknown {
  return db.pragma(pragma);
}

// ============================================================
// L. better-sqlite3-unique extensions
// ============================================================

@sugar.bind({
  concept: "concept:sql-result-mode",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
  loss: ["mode-is-stateful-on-stmt"],
})
export function stmtPluck(stmt: Database.Statement, enabled = true): Database.Statement {
  return stmt.pluck(enabled);
}

@sugar.bind({
  concept: "concept:sql-result-mode",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
  loss: ["mode-is-stateful-on-stmt", "expand-namespaces-columns"],
})
export function stmtExpand(stmt: Database.Statement, enabled = true): Database.Statement {
  return stmt.expand(enabled);
}

@sugar.bind({
  concept: "concept:sql-result-mode",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
  loss: ["mode-is-stateful-on-stmt", "raw-array-output"],
})
export function stmtRaw(stmt: Database.Statement, enabled = true): Database.Statement {
  return stmt.raw(enabled);
}

@sugar.bind({
  concept: "concept:sql-integer-mode",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
  loss: ["bigint-vs-number-switching"],
})
export function stmtSafeIntegers(stmt: Database.Statement, enabled = true): Database.Statement {
  return stmt.safeIntegers(enabled);
}

@sugar.bind({
  concept: "concept:sql-scalar-function",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
  loss: ["deterministic-flag", "varargs-flag", "direct-only-flag"],
})
export function dbFunction(db: Database.Database, name: string, callback: (...args: unknown[]) => unknown): void {
  db.function(name, callback as (...args: unknown[]) => unknown);
}

@sugar.bind({
  concept: "concept:sql-aggregate-function",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
  loss: ["step-result-final-step-shape"],
})
export function dbAggregate(db: Database.Database, name: string, options: Database.AggregateOptions): void {
  db.aggregate(name, options);
}

@sugar.bind({
  concept: "concept:sql-integer-mode",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
  loss: ["bigint-vs-number-switching"],
})
export function dbDefaultSafeIntegers(db: Database.Database, enabled = true): Database.Database {
  db.defaultSafeIntegers(enabled);
  return db;
}

@sugar.bind({
  concept: "concept:sql-serialize",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
  loss: ["attached-schema-name"],
})
export function dbSerialize(db: Database.Database): Buffer {
  return db.serialize();
}

@sugar.bind({
  concept: "concept:sql-unsafe-mode",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
  loss: ["unsafe-flag-semantics"],
})
export function dbUnsafeMode(db: Database.Database, enabled = true): Database.Database {
  db.unsafeMode(enabled);
  return db;
}

@sugar.bind({
  concept: "concept:sql-virtual-table",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
  loss: ["vtab-factory-shape", "eponymous-flag"],
})
export function dbTable(db: Database.Database, name: string, factory: Database.VirtualTableOptions): void {
  db.table(name, factory);
}

@sugar.bind({
  concept: "concept:sql-stmt-busy",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
  observed_dimension: "busy",
})
export function stmtBusy(stmt: Database.Statement): boolean {
  return stmt.busy;
}

@sugar.bind({
  concept: "concept:sql-stmt-readonly",
  library: "better-sqlite3",
  family: "concept:family:sql",
  version: "12.9.0",
  observed_dimension: "stmt-readonly",
})
export function stmtReadonly(stmt: Database.Statement): boolean {
  return stmt.readonly;
}

// ============================================================
// M. Refusals (10)
//    Each @sugar.refuse class carrier documents one concept whose
//    shape cannot be expressed by this library's synchronous API.
// ============================================================

@sugar.refuse({
  surface: "typescript-bind",
  concept: "concept:sql-physical-backup",
  reason: "db.backup() returns Promise<BackupMetadata>; the concept cluster requires a sync-shaped physical-backup primitive and the async shape cannot be losslessly transported",
  would_close_with_cluster: "concept:sql-physical-backup",
})
class RefusedBackup {}

@sugar.refuse({
  surface: "typescript-bind",
  concept: "concept:sql-blob-handle",
  reason: "better-sqlite3 has no incremental BLOB read/write API; blob_open is a SQLite C-level primitive not exposed by this driver",
  would_close_with_cluster: "concept:sql-blob-handle",
})
class RefusedBlobHandle {}

@sugar.refuse({
  surface: "typescript-bind",
  concept: "concept:dynamic-library-load",
  reason: "db.loadExtension() is an OS-tier binding; extension loading belongs at the OS-binding layer, not the SQL-driver layer",
  would_close_with_cluster: "concept:dynamic-library-load",
})
class RefusedLoadExtension {}

@sugar.refuse({
  surface: "typescript-bind",
  concept: "concept:sql-collation-register",
  reason: "better-sqlite3 exposes no collation registration API; the rusqlite create_collation callback shape has no equivalent surface here",
  would_close_with_cluster: "concept:sql-collation-register",
})
class RefusedCollation {}

@sugar.refuse({
  surface: "typescript-bind",
  concept: "concept:sql-busy-handler",
  reason: "better-sqlite3 does not expose a callback-shaped busy handler; only busy_timeout pragma is available, which is already bound",
  would_close_with_cluster: "concept:sql-busy-handler",
})
class RefusedBusyHandler {}

@sugar.refuse({
  surface: "typescript-bind",
  concept: "concept:sql-row-pointer-type",
  reason: "better-sqlite3 has no pointer-passing row type; SQLite pointer-passing is a C-level primitive not surfaced by this driver",
  would_close_with_cluster: "concept:sql-row-pointer-type",
})
class RefusedRowPointerType {}

@sugar.refuse({
  surface: "typescript-bind",
  concept: "concept:sql-transaction-commit",
  reason: "better-sqlite3 transactions commit implicitly when the transaction callback returns; there is no explicit commit() call available",
  would_close_with_cluster: "concept:sql-transaction-commit",
})
class RefusedExplicitCommit {}

@sugar.refuse({
  surface: "typescript-bind",
  concept: "concept:sql-transaction-rollback",
  reason: "better-sqlite3 transactions rollback implicitly on thrown exception; there is no explicit rollback() call available on the transaction object",
  would_close_with_cluster: "concept:sql-transaction-rollback",
})
class RefusedExplicitRollback {}

@sugar.refuse({
  surface: "typescript-bind",
  concept: "concept:sql-prepare-cached",
  reason: "better-sqlite3 statement objects are lightweight and the driver provides no prepare-cache or discard-cached API; statement reuse is caller-managed",
  would_close_with_cluster: "concept:sql-prepare-cached",
})
class RefusedPrepareCached {}

@sugar.refuse({
  surface: "typescript-bind",
  concept: "concept:sql-changes-total",
  reason: "better-sqlite3 exposes no total_changes counter; only per-statement changes is available through RunResult.changes",
  would_close_with_cluster: "concept:sql-changes-total",
})
class RefusedChangesTotal {}
