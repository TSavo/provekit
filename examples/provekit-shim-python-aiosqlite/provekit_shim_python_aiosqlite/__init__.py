# SPDX-License-Identifier: Apache-2.0
#
# provekit-shim-python-aiosqlite: substrate-honest concept bindings for the
# `aiosqlite` async SQLite driver.
#
# This module is the ASYNC mirror of provekit-shim-python-sqlite3. aiosqlite is
# a thin async wrapper over the Python stdlib sqlite3 module: its API is the
# same surface with `await`, `async with`, and async cursor iteration. Every
# claim this kit makes is in this file; there are no sidecar files. The
# substrate-uniform pattern is identical to the sqlite3 shim: the lift kit reads
# this source, extracts the structural shape of each annotated function body,
# attaches the per-binding loss declarations from the annotation arguments, and
# cmd_mint consumes the lift kit IR over JSON-RPC to produce a signed .proof
# envelope. The .proof is the source-of-truth for this kit's emission bodies
# (the old central python-canonical-bodies-aiosqlite.json was deleted in #1468),
# matching the sqlite3/better-sqlite3 pattern.
#
# Three speech acts per paper 24:
#   1. @sugar.bind(... loss=[])           materialize
#   2. @sugar.bind(... loss=["<dims>"])   loudly-bounded-lossy
#   3. @refuse(...)                       refuse with reason
#
# Async honesty (the one divergence from a mechanical sqlite3 mirror):
#   sqlite3 carries `sync-vs-async` as a loss dimension on every binding (it is
#   the sync driver, so async semantics are LOST). aiosqlite IS the async kit,
#   so `sync-vs-async` is NOT a loss here. The functions are `async def`; the
#   lifter records the async body shape; the realizer emits `async def`. Where a
#   sqlite3 binding declared `sync-vs-async`, the aiosqlite mirror drops it and
#   keeps the remaining (genuinely-lost) dimensions.
#
# CARDINALITY SPLIT (#1468): the connection/cursor query bindings cite the
# GLOBAL cardinality concepts (Phase 0 catalog) by post-condition, not a flat
# concept:sql-query:
#   * fetchone()-shaped (query_row, cursor_fetchone, migrate_query_row)
#                                     -> concept:sql-query-row   (one row or None)
#   * fetchall()-shaped (query_all, cursor_fetchall, migrate_query)
#                                     -> concept:sql-query-all   (materialized list)
#   * lazy-cursor (cursor_query / migrate_query_iterate: execute then return the
#     unconsumed cursor, no `async with` so the cursor outlives the call)
#                                     -> concept:sql-query-iterate (lazy single-pass)

import aiosqlite
from typing import Any, List, Optional, Tuple

from provekit import sugar, refuse

# =============================================================================
# A. Connection lifecycle
# =============================================================================

@sugar.bind(
    concept="concept:sql-connection-open",
    library="aiosqlite",
    family="concept:family:sql",
    version="0.20",
    loss=["auth-mechanism", "connection-pooling"],
)
async def open_db(path: str) -> aiosqlite.Connection:
    return await aiosqlite.connect(path)


@sugar.bind(
    concept="concept:sql-connection-open",
    library="aiosqlite",
    family="concept:family:sql",
    version="0.20",
    loss=["persistence-target"],
)
async def open_in_memory() -> aiosqlite.Connection:
    return await aiosqlite.connect(":memory:")


@sugar.bind(
    concept="concept:sql-connection-close",
    library="aiosqlite",
    family="concept:family:sql",
    version="0.20",
    loss=["ownership-model"],
)
async def close_connection(conn: aiosqlite.Connection) -> None:
    await conn.close()


# =============================================================================
# B. Query execution at the Connection level
# =============================================================================

@sugar.bind(
    concept="concept:sql-execute",
    library="aiosqlite",
    family="concept:family:sql",
    version="0.20",
    loss=["last-insert-id", "transaction-isolation", "row-typing-mode"],
)
async def execute(conn: aiosqlite.Connection, sql: str, params: Any = ()) -> aiosqlite.Cursor:
    return await conn.execute(sql, params)


@sugar.bind(
    concept="concept:sql-batch-execute",
    library="aiosqlite",
    family="concept:family:sql",
    version="0.20",
    loss=["atomicity-across-statements", "parameter-binding"],
)
async def executescript(conn: aiosqlite.Connection, sql_script: str) -> aiosqlite.Cursor:
    return await conn.executescript(sql_script)


@sugar.bind(
    concept="concept:sql-batch-execute",
    library="aiosqlite",
    family="concept:family:sql",
    version="0.20",
    loss=["atomicity-across-statements"],
)
async def executemany(conn: aiosqlite.Connection, sql: str, seq_of_params: Any) -> aiosqlite.Cursor:
    return await conn.executemany(sql, seq_of_params)


@sugar.bind(
    concept="concept:sql-query-row",
    library="aiosqlite",
    family="concept:family:sql",
    version="0.20",
    loss=["row-typing-mode", "cursor-lifetime"],
)
async def query_row(conn: aiosqlite.Connection, sql: str, params: Any = ()) -> Optional[Tuple]:
    async with conn.execute(sql, params) as cursor:
        return await cursor.fetchone()


@sugar.bind(
    concept="concept:sql-query-all",
    library="aiosqlite",
    family="concept:family:sql",
    version="0.20",
    loss=["row-typing-mode", "cursor-lifetime"],
)
async def query_all(conn: aiosqlite.Connection, sql: str, params: Any = ()) -> List[Tuple]:
    async with conn.execute(sql, params) as cursor:
        return await cursor.fetchall()


# =============================================================================
# C. Statement preparation (Cursor)
# =============================================================================

@sugar.bind(
    concept="concept:sql-prepare",
    library="aiosqlite",
    family="concept:family:sql",
    version="0.20",
    loss=["parameter-style", "ownership-model"],
)
async def prepare(conn: aiosqlite.Connection, sql: str) -> aiosqlite.Cursor:
    return await conn.cursor()


# =============================================================================
# D. Statement execution (via Cursor)
# =============================================================================

@sugar.bind(
    concept="concept:sql-execute",
    library="aiosqlite",
    family="concept:family:sql",
    version="0.20",
    loss=["last-insert-id", "transaction-isolation", "row-typing-mode"],
)
async def cursor_execute(cursor: aiosqlite.Cursor, sql: str, params: Any = ()) -> aiosqlite.Cursor:
    return await cursor.execute(sql, params)


@sugar.bind(
    concept="concept:sql-query-iterate",
    library="aiosqlite",
    family="concept:family:sql",
    version="0.20",
    loss=["row-typing-mode", "cursor-lifetime"],
)
async def cursor_query(cursor: aiosqlite.Cursor, sql: str, params: Any = ()) -> aiosqlite.Cursor:
    # Lazy single-pass: execute then return the LIVE cursor (no `async with`,
    # which would close it on block exit and break caller iteration).
    await cursor.execute(sql, params)
    return cursor


@sugar.bind(
    concept="concept:sql-query-row",
    library="aiosqlite",
    family="concept:family:sql",
    version="0.20",
    loss=["row-typing-mode", "cursor-lifetime"],
)
async def cursor_fetchone(cursor: aiosqlite.Cursor) -> Optional[Tuple]:
    return await cursor.fetchone()


@sugar.bind(
    concept="concept:sql-query-all",
    library="aiosqlite",
    family="concept:family:sql",
    version="0.20",
    loss=["row-typing-mode", "cursor-lifetime"],
)
async def cursor_fetchall(cursor: aiosqlite.Cursor) -> List[Tuple]:
    return await cursor.fetchall()


@sugar.bind(
    concept="concept:sql-fetch-batch",
    library="aiosqlite",
    family="concept:family:sql",
    version="0.20",
    loss=["row-typing-mode", "cursor-pagination"],
)
async def cursor_fetchmany(cursor: aiosqlite.Cursor, size: Optional[int] = None) -> List[Tuple]:
    if size is None:
        return await cursor.fetchmany()
    return await cursor.fetchmany(size)


@sugar.bind(
    concept="concept:insert-and-get-id",
    library="aiosqlite",
    family="concept:family:sql",
    version="0.20",
    loss=["id-column-discovery", "row-typing-mode"],
)
async def cursor_execute_and_lastrowid(cursor: aiosqlite.Cursor, sql: str, params: Any = ()) -> Optional[int]:
    await cursor.execute(sql, params)
    return cursor.lastrowid


# =============================================================================
# E. Transaction control
# =============================================================================

@sugar.bind(
    concept="concept:sql-transaction-begin",
    library="aiosqlite",
    family="concept:family:sql",
    version="0.20",
    loss=["isolation-level", "ownership-model"],
)
async def begin_transaction(conn: aiosqlite.Connection) -> None:
    await conn.execute("BEGIN")


@sugar.bind(
    concept="concept:sql-transaction-commit",
    library="aiosqlite",
    family="concept:family:sql",
    version="0.20",
    loss=["two-phase-commit-support", "ownership-model"],
)
async def commit(conn: aiosqlite.Connection) -> None:
    await conn.commit()


@sugar.bind(
    concept="concept:sql-transaction-rollback",
    library="aiosqlite",
    family="concept:family:sql",
    version="0.20",
    loss=["partial-rollback-support", "ownership-model"],
)
async def rollback(conn: aiosqlite.Connection) -> None:
    await conn.rollback()


# =============================================================================
# F. Changes counting
# =============================================================================

@sugar.bind(
    concept="concept:insert-and-get-id",
    library="aiosqlite",
    family="concept:family:sql",
    version="0.20",
    loss=["per-connection-not-per-statement", "rowid-vs-integer-pk"],
)
async def last_insert_rowid(cursor: aiosqlite.Cursor) -> Optional[int]:
    return cursor.lastrowid


@sugar.bind(
    concept="concept:sql-changes-count",
    library="aiosqlite",
    family="concept:family:sql",
    version="0.20",
    loss=["per-statement-vs-cumulative", "transaction-scope"],
)
async def rowcount(cursor: aiosqlite.Cursor) -> int:
    return cursor.rowcount


@sugar.bind(
    concept="concept:sql-changes-count",
    library="aiosqlite",
    family="concept:family:sql",
    version="0.20",
    loss=["cumulative-since-connection-open", "transaction-scope"],
)
async def total_changes(conn: aiosqlite.Connection) -> int:
    return conn.total_changes


# =============================================================================
# G. Row reading
# =============================================================================

@sugar.bind(
    concept="concept:sql-row-get-column",
    library="aiosqlite",
    family="concept:family:sql",
    version="0.20",
    loss=["null-handling", "type-coercion-mode", "row-typing-mode"],
)
async def row_get_by_index(row: Tuple, idx: int) -> Any:
    return row[idx]


@sugar.bind(
    concept="concept:sql-row-mapping",
    library="aiosqlite",
    family="concept:family:sql",
    version="0.20",
    loss=["row-typing-mode", "ownership-model"],
)
async def set_row_factory(conn: aiosqlite.Connection) -> None:
    conn.row_factory = aiosqlite.Row


# =============================================================================
# P. Migrate-shaped 2-param SQL bindings (#1468)
# =============================================================================
#
# The typescript-better-sqlite3 -> python-aiosqlite migrate probes the SQL read /
# write / insert concepts at the 2-param ["string","unknown[]"] arity that
# better-sqlite3's db.prepare(q).{get,all,iterate,run}(p) lifts to (substrate-
# availability probe, #1230 D6-D). Post the cardinality split (#1468), the read
# concept is selected by result cardinality, so the migrate path needs a 2-param
# (sql, args) binding for EACH cardinality the better-sqlite3 source can produce
# at a migrate callsite:
#   * .all(p)  / fetchall() -> concept:sql-query-all    (migrate_query)
#   * .get(p)  / fetchone() -> concept:sql-query-row    (migrate_query_row)
#   * .iterate(p) / cursor  -> concept:sql-query-iterate (migrate_query_iterate)
#   * .run(p)  + lastInsertRowid -> concept:insert-and-get-id (migrate_insert_and_get_id)
#   * .run(p)               -> concept:sql-execute      (migrate_execute)
# The regular query_row/cursor_fetchone bindings are arity-1/arity-3 (the wrong
# shape for the migrate probe, which is fixed arity-2), so the migrate trio mints
# its own per-cardinality siblings. The connection is a free `db` binding the
# migrate assembler hoists (not a method receiver), and the args list is bound by
# position then tuple()-wrapped for the aiosqlite driver. The free `db`/`cursor`
# names pass through the param->placeholder projection unchanged; only
# `sql`/`args` map to ${param0}/${param1}. Mirrors the sqlite3 shim's section P
# with async (`await` / `async with`) bodies.

@sugar.bind(
    concept="concept:sql-execute",
    library="aiosqlite",
    family="concept:family:sql",
    version="0.20",
    loss=["last-insert-id", "transaction-isolation", "row-typing-mode"],
)
async def migrate_execute(sql, args):
    async with db.execute(sql, tuple(args)) as cursor:
        await db.commit()
        return {"rows_affected": cursor.rowcount, "last_insert_id": cursor.lastrowid}


@sugar.bind(
    concept="concept:insert-and-get-id",
    library="aiosqlite",
    family="concept:family:sql",
    version="0.20",
    loss=["id-column-discovery", "row-typing-mode"],
)
async def migrate_insert_and_get_id(sql, args):
    cursor = await db.execute(sql, tuple(args))
    await db.commit()
    return int(cursor.lastrowid or 0)


@sugar.bind(
    concept="concept:sql-query-all",
    library="aiosqlite",
    family="concept:family:sql",
    version="0.20",
    loss=["row-typing-mode", "cursor-lifetime"],
)
async def migrate_query(sql, args):
    async with db.execute(sql, tuple(args)) as cursor:
        return await cursor.fetchall()


@sugar.bind(
    concept="concept:sql-query-row",
    library="aiosqlite",
    family="concept:family:sql",
    version="0.20",
    loss=["row-typing-mode", "cursor-lifetime"],
)
async def migrate_query_row(sql, args):
    async with db.execute(sql, tuple(args)) as cursor:
        return await cursor.fetchone()


@sugar.bind(
    concept="concept:sql-query-iterate",
    library="aiosqlite",
    family="concept:family:sql",
    version="0.20",
    loss=["row-typing-mode", "cursor-lifetime"],
)
async def migrate_query_iterate(sql, args):
    cursor = await db.execute(sql, tuple(args))
    return cursor


# =============================================================================
# Refusals
# =============================================================================
#
# Each refusal is a signed signpost. The substrate publishes the demand the
# shim declines to fill, naming the cluster constraint that would close it.

@refuse(
    surface="aiosqlite.Connection.backup",
    concept="concept:sql-physical-backup",
    reason="SQLite-binary-specific physical backup. Postgres has pg_basebackup (out-of-band, not a connection method); MySQL has equivalent. N=1 across connection-level APIs for now; cluster does not yet form. Refusing rather than minting a single-kit concept hub.",
    would_close_with_cluster="Connection-level physical-backup method on >=2 SQL drivers",
)
class RefusedBackup:
    pass


@refuse(
    surface="aiosqlite.enable_load_extension",
    concept="concept:dynamic-library-load",
    reason="OS-level dynamic-library-load, not SQL-domain. The right concept lives at the OS-binding tier (Python ctypes, Rust libloading, C dlopen, etc.) not at the SQL kit tier. enable_load_extension also requires a compile-time flag in the underlying CPython sqlite3 build; substrate-honest discipline is to refuse rather than bind a conditionally-available API.",
    would_close_with_cluster="OS-tier kit minting (separate from SQL-driver-tier)",
)
class RefusedLoadExtension:
    pass
