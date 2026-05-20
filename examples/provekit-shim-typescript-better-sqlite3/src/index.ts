// SPDX-License-Identifier: Apache-2.0
//
// @provekit-shim/typescript-better-sqlite3: substrate-honest concept
// bindings for the synchronous better-sqlite3 Node driver.
//
// This package mirrors the vendored-boundary namespace pattern used by
// provekit-shim-rusqlite, but uses the TypeScript source-native
// `@sugar.bind(...)` surface. The TypeScript lifter reads this file with
// `layer = "library-bindings"`, emits `library-sugar-binding-entry` IR, and
// the shared Rust `provekit mint --library-bindings` path packages those
// entries into the package-root `provekit.proof` envelope.

import Database from "better-sqlite3";
import { sugar } from "provekit";

export type Params = readonly unknown[] | Record<string, unknown>;

@sugar.bind({
  concept: "concept:sql-connection-open",
  library: "better-sqlite3",
})
export function open(filename: string): Database.Database {
  return new Database(filename);
}

@sugar.bind({
  concept: "concept:sql-connection-open",
  library: "better-sqlite3",
})
export function openInMemory(): Database.Database {
  return new Database(":memory:");
}

@sugar.bind({
  concept: "concept:sql-execute",
  library: "better-sqlite3",
})
export function execute(db: Database.Database, sql: string, params: Params = []): Database.RunResult {
  return db.prepare(sql).run(params);
}

@sugar.bind({
  concept: "concept:sql-query",
  library: "better-sqlite3",
})
export function allRows(db: Database.Database, sql: string, params: Params = []): unknown[] {
  return db.prepare(sql).all(params);
}

@sugar.bind({
  concept: "concept:sql-query",
  library: "better-sqlite3",
})
export function getRow(db: Database.Database, sql: string, params: Params = []): unknown {
  return db.prepare(sql).get(params);
}

@sugar.bind({
  concept: "concept:insert-and-get-id",
  library: "better-sqlite3",
})
export function lastInsertRowid(result: Database.RunResult): number | bigint {
  return result.lastInsertRowid;
}

@sugar.bind({
  concept: "concept:sql-transaction-begin",
  library: "better-sqlite3",
})
export function transaction<T>(db: Database.Database, body: () => T): T {
  return db.transaction(body)();
}

@sugar.bind({
  concept: "concept:sql-connection-close",
  library: "better-sqlite3",
})
export function close(db: Database.Database): void {
  db.close();
}
