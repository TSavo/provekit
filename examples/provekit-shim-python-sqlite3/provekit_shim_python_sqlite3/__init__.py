# SPDX-License-Identifier: Apache-2.0
#
# provekit-shim-python-sqlite3: substrate-honest concept bindings for the
# Python stdlib sqlite3 module.
#
# This module is the first Python vendored boundary namespace under the ProvekIt
# proofchain (paper 03: substrate, not blockchain; paper 24: the proofchain is
# the exchange). Every claim this kit makes is in this file. There are no sidecar
# files. The substrate-uniform pattern is: the lift kit reads this source,
# extracts the structural shape of each annotated function body, attaches the
# per-binding loss declarations directly from the annotation arguments, attaches
# the observed_dimension for observation bindings, and emits refusal-memento IR
# for each @refuse annotation. cmd_mint consumes the lift kit IR over JSON-RPC
# and produces a signed .proof envelope.
#
# Three speech acts per paper 24:
#   1. @sugar.bind(... loss=[])           materialize
#   2. @sugar.bind(... loss=["<dims>"])   loudly-bounded-lossy
#   3. @refuse(...)                       refuse with reason
#
# Design choices vs. rusqlite (paper 24 s3):
#   - OVERLAP: every rusqlite sugar concept (23 unique concept names) is
#     represented here, with the same concept name to enable cluster formation.
#   - EXTENSIONS: Python sqlite3 has unique surface not in rusqlite:
#     Connection.iterdump(), set_progress_handler(), create_function(),
#     Cursor.fetchmany(), executemany() / executescript(), row_factory.
#   - LOSS DIMENSIONS DIFFER from rusqlite where Python semantics diverge:
#       ownership-model: sqlite3 uses GC references, not borrow checker; wherever
#         rusqlite declares lifetime/borrow, sqlite3 carries ownership-model.
#       row-typing-mode: sqlite3 rows are tuples or Row objects (dynamic),
#         not statically typed via FromSql; wherever rusqlite has typed get<T>
#         calls, sqlite3 carries row-typing-mode.
#       sync-vs-async: sqlite3 is sync (matches rusqlite).
#   - CONCEPT COUNT: 50 sugar bindings across 26 unique concept names, plus
#     10 refusals. Total envelope members: 60.
#   - CARDINALITY SPLIT (#1468): the connection/cursor query bindings cite the
#     GLOBAL cardinality concepts (Phase 0 catalog) by post-condition, not a flat
#     concept:sql-query:
#       * fetchone()-shaped (query_row, cursor_fetchone, cursor_exists)
#                                         -> concept:sql-query-row   (one row or None)
#       * fetchall()-shaped (query_all, cursor_fetchall, migrate_query)
#                                         -> concept:sql-query-all   (materialized list)
#       * lazy-cursor (cursor_query: execute then return the unconsumed cursor)
#                                         -> concept:sql-query-iterate (lazy single-pass)
#     cursor_fetchmany stays on concept:sql-fetch-batch (size-bounded page, distinct).

import sqlite3
from typing import Any, Callable, Iterator, List, Optional, Tuple

from provekit import sugar, refuse

# =============================================================================
# A. Connection lifecycle
# =============================================================================

@sugar.bind(
    concept="concept:sql-connection-open",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "auth-mechanism", "connection-pooling"],
)
def open_db(path: str) -> sqlite3.Connection:
    return sqlite3.connect(path)


@sugar.bind(
    concept="concept:sql-connection-open",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "persistence-target"],
)
def open_in_memory() -> sqlite3.Connection:
    return sqlite3.connect(":memory:")


@sugar.bind(
    concept="concept:sql-connection-open",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "auth-mechanism", "connection-pooling", "flag-encoding"],
)
def open_with_uri(uri: str) -> sqlite3.Connection:
    # sqlite3 supports URI filenames via detect_types=sqlite3.PARSE_DECLTYPES;
    # the closest flag-encoding equivalent is URI mode (check_same_thread, timeout, etc.)
    return sqlite3.connect(uri, check_same_thread=False)


@sugar.bind(
    concept="concept:sql-connection-close",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "ownership-model"],
)
def close_connection(conn: sqlite3.Connection) -> None:
    conn.close()


# =============================================================================
# B. Query execution at the Connection level
# =============================================================================

@sugar.bind(
    concept="concept:sql-execute",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "last-insert-id", "transaction-isolation", "row-typing-mode"],
)
def execute(conn: sqlite3.Connection, sql: str, params: Any = ()) -> sqlite3.Cursor:
    return conn.execute(sql, params)


@sugar.bind(
    concept="concept:sql-batch-execute",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "atomicity-across-statements", "parameter-binding"],
)
def executescript(conn: sqlite3.Connection, sql_script: str) -> sqlite3.Cursor:
    return conn.executescript(sql_script)


@sugar.bind(
    concept="concept:sql-batch-execute",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "atomicity-across-statements"],
)
def executemany(conn: sqlite3.Connection, sql: str, seq_of_params: Any) -> sqlite3.Cursor:
    return conn.executemany(sql, seq_of_params)


@sugar.bind(
    concept="concept:sql-query-row",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "row-typing-mode", "cursor-lifetime"],
)
def query_row(conn: sqlite3.Connection, sql: str, params: Any = ()) -> Optional[Tuple]:
    cursor = conn.execute(sql, params)
    return cursor.fetchone()


@sugar.bind(
    concept="concept:sql-query-all",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "row-typing-mode", "cursor-lifetime"],
)
def query_all(conn: sqlite3.Connection, sql: str, params: Any = ()) -> List[Tuple]:
    cursor = conn.execute(sql, params)
    return cursor.fetchall()


# =============================================================================
# C. Statement preparation (Cursor)
# =============================================================================

@sugar.bind(
    concept="concept:sql-prepare",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "parameter-style", "ownership-model"],
)
def prepare(conn: sqlite3.Connection, sql: str) -> sqlite3.Cursor:
    return conn.cursor()


@sugar.bind(
    concept="concept:sql-prepare-cached",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "cache-eviction-policy", "cache-size-bound", "ownership-model"],
)
def prepare_cached(conn: sqlite3.Connection, sql: str) -> sqlite3.Cursor:
    # sqlite3 caches statements internally; no Python-level prepare_cached API.
    # The closest approximation: create a cursor and note the Python binding carries
    # cache-eviction-policy loss (Python's internal C cache is not user-controlled).
    return conn.cursor()


# =============================================================================
# D. Statement execution (via Cursor)
# =============================================================================

@sugar.bind(
    concept="concept:sql-execute",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "last-insert-id", "transaction-isolation", "row-typing-mode"],
)
def cursor_execute(cursor: sqlite3.Cursor, sql: str, params: Any = ()) -> sqlite3.Cursor:
    return cursor.execute(sql, params)


@sugar.bind(
    concept="concept:sql-query-iterate",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "row-typing-mode", "cursor-lifetime"],
)
def cursor_query(cursor: sqlite3.Cursor, sql: str, params: Any = ()) -> sqlite3.Cursor:
    cursor.execute(sql, params)
    return cursor


@sugar.bind(
    concept="concept:sql-query-row",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "row-typing-mode", "cursor-lifetime"],
)
def cursor_fetchone(cursor: sqlite3.Cursor) -> Optional[Tuple]:
    return cursor.fetchone()


@sugar.bind(
    concept="concept:sql-query-all",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "row-typing-mode", "cursor-lifetime"],
)
def cursor_fetchall(cursor: sqlite3.Cursor) -> List[Tuple]:
    return cursor.fetchall()


@sugar.bind(
    concept="concept:sql-fetch-batch",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "row-typing-mode", "cursor-pagination"],
)
def cursor_fetchmany(cursor: sqlite3.Cursor, size: Optional[int] = None) -> List[Tuple]:
    if size is None:
        return cursor.fetchmany()
    return cursor.fetchmany(size)


@sugar.bind(
    concept="concept:insert-and-get-id",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "id-column-discovery", "row-typing-mode"],
)
def cursor_execute_and_lastrowid(cursor: sqlite3.Cursor, sql: str, params: Any = ()) -> Optional[int]:
    cursor.execute(sql, params)
    return cursor.lastrowid


@sugar.bind(
    concept="concept:sql-query-row",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "cardinality-projected-to-boolean", "row-typing-mode"],
)
def cursor_exists(cursor: sqlite3.Cursor, sql: str, params: Any = ()) -> bool:
    cursor.execute(sql, params)
    return cursor.fetchone() is not None


# =============================================================================
# E. Transaction control
# =============================================================================

@sugar.bind(
    concept="concept:sql-transaction-begin",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "isolation-level", "ownership-model"],
)
def begin_transaction(conn: sqlite3.Connection) -> None:
    conn.execute("BEGIN")


@sugar.bind(
    concept="concept:sql-transaction-begin",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "isolation-level", "deferred-vs-immediate-vs-exclusive", "ownership-model"],
)
def begin_transaction_with_behavior(conn: sqlite3.Connection, behavior: str) -> None:
    # behavior: "DEFERRED", "IMMEDIATE", or "EXCLUSIVE"
    conn.execute(f"BEGIN {behavior}")


@sugar.bind(
    concept="concept:sql-transaction-commit",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "two-phase-commit-support", "ownership-model"],
)
def commit(conn: sqlite3.Connection) -> None:
    conn.commit()


@sugar.bind(
    concept="concept:sql-transaction-rollback",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "partial-rollback-support", "ownership-model"],
)
def rollback(conn: sqlite3.Connection) -> None:
    conn.rollback()


@sugar.bind(
    concept="concept:sql-savepoint",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["naming-discipline", "ownership-model"],
)
def savepoint(conn: sqlite3.Connection, name: str) -> None:
    conn.execute(f"SAVEPOINT {name}")


@sugar.bind(
    concept="concept:sql-transaction-rollback",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["partial-rollback-support"],
)
def rollback_to_savepoint(conn: sqlite3.Connection, name: str) -> None:
    conn.execute(f"ROLLBACK TO SAVEPOINT {name}")


@sugar.bind(
    concept="concept:sql-savepoint",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["naming-discipline", "ownership-model"],
)
def release_savepoint(conn: sqlite3.Connection, name: str) -> None:
    conn.execute(f"RELEASE SAVEPOINT {name}")


# =============================================================================
# F. Row reading
# =============================================================================

@sugar.bind(
    concept="concept:sql-row-get-column",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["null-handling", "type-coercion-mode", "row-typing-mode"],
)
def row_get_by_index(row: Tuple, idx: int) -> Any:
    return row[idx]


@sugar.bind(
    concept="concept:sql-row-get-column",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["null-handling", "type-coercion-mode", "row-typing-mode"],
)
def row_get_by_name(row: sqlite3.Row, name: str) -> Any:
    return row[name]


@sugar.bind(
    concept="concept:sql-row-mapping",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["row-typing-mode", "ownership-model"],
)
def set_row_factory(conn: sqlite3.Connection) -> None:
    conn.row_factory = sqlite3.Row


# =============================================================================
# G. Changes counting
# =============================================================================

@sugar.bind(
    concept="concept:insert-and-get-id",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["per-connection-not-per-statement", "rowid-vs-integer-pk"],
)
def last_insert_rowid(cursor: sqlite3.Cursor) -> Optional[int]:
    return cursor.lastrowid


@sugar.bind(
    concept="concept:sql-changes-count",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["per-statement-vs-cumulative", "transaction-scope"],
)
def rowcount(cursor: sqlite3.Cursor) -> int:
    return cursor.rowcount


@sugar.bind(
    concept="concept:sql-changes-count",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["cumulative-since-connection-open", "transaction-scope"],
)
def total_changes(conn: sqlite3.Connection) -> int:
    return conn.total_changes


# =============================================================================
# H. Connection state observation
# =============================================================================

@sugar.bind(
    concept="concept:contract-observation",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    observed_dimension="autocommit-mode",
)
def in_transaction(conn: sqlite3.Connection) -> bool:
    return conn.in_transaction


@sugar.bind(
    concept="concept:contract-observation",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    observed_dimension="isolation-level",
)
def isolation_level(conn: sqlite3.Connection) -> Optional[str]:
    return conn.isolation_level


# =============================================================================
# I. Statement (Cursor) metadata observation
# =============================================================================

@sugar.bind(
    concept="concept:contract-observation",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    observed_dimension="column-names",
)
def cursor_column_names(cursor: sqlite3.Cursor) -> Optional[Tuple]:
    return cursor.description


@sugar.bind(
    concept="concept:contract-observation",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    observed_dimension="column-count",
)
def cursor_column_count(cursor: sqlite3.Cursor) -> int:
    if cursor.description is None:
        return 0
    return len(cursor.description)


@sugar.bind(
    concept="concept:contract-observation",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    observed_dimension="column-index-of-name",
)
def cursor_column_index(cursor: sqlite3.Cursor, name: str) -> Optional[int]:
    if cursor.description is None:
        return None
    for idx, col in enumerate(cursor.description):
        if col[0] == name:
            return idx
    return None


# =============================================================================
# J. Concurrency control
# =============================================================================

@sugar.bind(
    concept="concept:sql-busy-timeout",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "callback-vs-timeout-shape"],
)
def set_timeout(conn: sqlite3.Connection, timeout_secs: float) -> None:
    # sqlite3.connect(timeout=...) sets this at open time; re-create
    # connection with new timeout or use PRAGMA busy_timeout (milliseconds).
    conn.execute(f"PRAGMA busy_timeout = {int(timeout_secs * 1000)}")


# =============================================================================
# K. Schema dump (Python-unique surface, N=1 carrier)
# =============================================================================

@sugar.bind(
    concept="concept:sql-schema-dump",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "iteration-vs-string-output"],
)
def iterdump(conn: sqlite3.Connection) -> Iterator[str]:
    return conn.iterdump()


# =============================================================================
# L. User-defined functions (Python-unique surface, N=1 carrier)
# =============================================================================

@sugar.bind(
    concept="concept:sql-udf-register",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "type-coercion-mode", "determinism-flag"],
)
def create_function(conn: sqlite3.Connection, name: str, num_params: int, func: Callable) -> None:
    conn.create_function(name, num_params, func)


@sugar.bind(
    concept="concept:sql-udf-aggregate",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "type-coercion-mode"],
)
def create_aggregate(conn: sqlite3.Connection, name: str, num_params: int, aggregate_class: type) -> None:
    conn.create_aggregate(name, num_params, aggregate_class)


# =============================================================================
# M. Progress handler (Python-unique surface, N=1 carrier)
# =============================================================================

@sugar.bind(
    concept="concept:sql-progress-handler",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "handler-frequency-semantics"],
)
def set_progress_handler(conn: sqlite3.Connection, handler: Optional[Callable], n: int) -> None:
    conn.set_progress_handler(handler, n)


# =============================================================================
# N. Context manager (Python-unique: with-statement transaction, N=1 carrier)
# =============================================================================

@sugar.bind(
    concept="concept:sql-transaction-begin",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "isolation-level", "ownership-model", "context-manager-lifetime"],
)
def connection_as_context_manager(conn: sqlite3.Connection) -> sqlite3.Connection:
    # Python sqlite3 supports: with conn: ... which auto-commits or rolls back.
    # The binding captures the entry point of the with-statement protocol.
    return conn.__enter__()


@sugar.bind(
    concept="concept:sql-transaction-commit",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "ownership-model", "context-manager-lifetime"],
)
def connection_context_exit_commit(conn: sqlite3.Connection) -> None:
    # On clean exit from `with conn:` block, sqlite3 commits automatically.
    conn.__exit__(None, None, None)


@sugar.bind(
    concept="concept:sql-transaction-rollback",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "ownership-model", "context-manager-lifetime"],
)
def connection_context_exit_rollback(conn: sqlite3.Connection, exc_type: Any) -> bool:
    # On exception exit from `with conn:` block, sqlite3 rolls back automatically.
    return conn.__exit__(exc_type, None, None) or False


@sugar.bind(
    concept="concept:sql-row-mapping",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["row-typing-mode", "ownership-model"],
)
def connection_row_factory_callable(conn: sqlite3.Connection, factory: Callable) -> None:
    # Allows setting a custom row factory (e.g., dict-based rows).
    conn.row_factory = factory


# =============================================================================
# P. Migrate-shaped 2-param SQL bindings (#1451)
# =============================================================================
#
# The typescript-better-sqlite3 -> python-sqlite3 migrate probes the SQL read /
# write / insert concepts at the 2-param ["string","unknown[]"] arity that
# better-sqlite3's db.prepare(q).{get,all,iterate}(p) lifts to (substrate-
# availability probe, #1230 D6-D). Post the cardinality split (#1468), the read
# concept is no longer flat concept:sql-query: it is selected by result
# cardinality, so the migrate path needs a 2-param (sql, args) binding for EACH
# cardinality the better-sqlite3 source can produce at a migrate callsite:
#   * .all(p)  / fetchall() -> concept:sql-query-all    (migrate_query)
#   * .get(p)  / fetchone() -> concept:sql-query-row    (migrate_query_row)
#   * .iterate(p) / cursor  -> concept:sql-query-iterate (migrate_query_iterate)
# The regular query_row/cursor_fetchone bindings are arity-1/arity-3 (the wrong
# shape for the migrate probe, which is fixed arity-2), so the migrate trio mints
# its own per-cardinality siblings. They mirror the sibling aiosqlite spec's trio
# with synchronous sqlite3 templates: the connection is a free `db` binding the
# migrate assembler hoists (not a method receiver), and the args list is bound by
# position then tuple()-wrapped for the sqlite3 driver. Originally back-propagated
# from python-canonical-bodies-sqlite3.json (authored by #1451 to green
# cross_platform_point_query_receipt_test, never into this source); the row/iterate
# siblings are added here directly (#1468) so the .proof carries them. The free
# `db`/`cursor` names pass through the param->placeholder projection unchanged;
# only `sql`/`args` map to ${param0}/${param1}.

@sugar.bind(
    concept="concept:sql-execute",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "last-insert-id", "transaction-isolation", "row-typing-mode"],
)
def migrate_execute(sql, args):
    cursor = db.execute(sql, tuple(args))
    db.commit()
    return {"rows_affected": cursor.rowcount, "last_insert_id": cursor.lastrowid}


@sugar.bind(
    concept="concept:insert-and-get-id",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "id-column-discovery", "row-typing-mode"],
)
def migrate_insert_and_get_id(sql, args):
    cursor = db.execute(sql, tuple(args))
    db.commit()
    return int(cursor.lastrowid or 0)


@sugar.bind(
    concept="concept:sql-query-all",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "row-typing-mode", "cursor-lifetime"],
)
def migrate_query(sql, args):
    cursor = db.execute(sql, tuple(args))
    return cursor.fetchall()


@sugar.bind(
    concept="concept:sql-query-row",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "row-typing-mode", "cursor-lifetime"],
)
def migrate_query_row(sql, args):
    cursor = db.execute(sql, tuple(args))
    return cursor.fetchone()


@sugar.bind(
    concept="concept:sql-query-iterate",
    library="sqlite3",
    family="concept:family:sql",
    version="python-3",
    loss=["sync-vs-async", "row-typing-mode", "cursor-lifetime"],
)
def migrate_query_iterate(sql, args):
    cursor = db.execute(sql, tuple(args))
    return cursor


# =============================================================================
# Refusals
# =============================================================================
#
# Each refusal is a signed signpost. The substrate publishes the demand the
# shim declines to fill, naming the cluster constraint that would close it.
# The lift kit emits a refusal-memento IR record per @refuse annotation;
# cmd_mint signs each as a RefusalMemento envelope member.

@refuse(
    surface="sqlite3.Connection.backup",
    concept="concept:sql-physical-backup",
    reason="SQLite-binary-specific physical backup. Postgres has pg_basebackup (out-of-band, not a connection method); MySQL has equivalent. N=1 across connection-level APIs for now; cluster does not yet form. Refusing rather than minting a single-kit concept hub.",
    would_close_with_cluster="Connection-level physical-backup method on >=2 SQL drivers",
)
class RefusedBackup:
    pass


@refuse(
    surface="sqlite3.Connection.open_blob",
    concept="concept:sql-blob-handle",
    reason="Incremental BLOB I/O is not exposed as a Python stdlib sqlite3 API at all. Python sqlite3 reads BLOBs as bytes objects in one shot. Postgres has lo_open with a different lifecycle model. The semantic shapes diverge enough that a single-kit binding would not serve cross-library composition.",
    would_close_with_cluster="Incremental BLOB I/O on >=2 SQL drivers with structurally compatible handle semantics",
)
class RefusedBlobHandle:
    pass


@refuse(
    surface="sqlite3.enable_load_extension",
    concept="concept:dynamic-library-load",
    reason="OS-level dynamic-library-load, not SQL-domain. The right concept lives at the OS-binding tier (Python ctypes, Rust libloading, C dlopen, etc.) not at the SQL kit tier. Also: enable_load_extension requires a compile-time flag in CPython; substrate-honest discipline is to refuse rather than bind a conditionally-available API.",
    would_close_with_cluster="OS-tier kit minting (separate from SQL-driver-tier)",
)
class RefusedLoadExtension:
    pass


@refuse(
    surface="sqlite3.Connection.create_collation",
    concept="concept:sql-collation-register",
    reason="Custom string-comparison callback registration. Postgres supports custom collations via CREATE COLLATION SQL; sqlite3 registers them as Python callables. The mechanism diverges enough that a single-kit binding would carry opaque callback semantics. The Python-specific surface (callable vs. Rust closure) further complicates cluster formation.",
    would_close_with_cluster="Custom collation registration on >=2 SQL drivers with structurally compatible callback semantics",
)
class RefusedCreateCollation:
    pass


@refuse(
    surface="sqlite3.Connection.set_authorizer",
    concept="concept:sql-busy-handler",
    reason="sqlite3 set_authorizer is a security callback for authorizing SQL operations, not a busy-collision handler. The concept:sql-busy-handler as used in rusqlite (Connection.busy_handler) is a dynamic callback for lock contention. These are structurally distinct callbacks at different lifecycle points. The Python sqlite3 module has no busy-handler callback (only the timeout-shaped busy_timeout via PRAGMA). Refusing the callback-shaped variant.",
    would_close_with_cluster="Callback-based busy-collision handling on >=2 SQL drivers",
)
class RefusedBusyHandler:
    pass


@refuse(
    surface="sqlite3.Row.__getitem__",
    concept="concept:sql-row-pointer-type",
    reason="Pointer-passing through SQLite's auxiliary data interface is not exposed in Python's sqlite3 binding. The Row object provides only value access. SQLite-specific feature; not a concept the substrate carries today for Python.",
    would_close_with_cluster="Pointer-passing row column type on >=2 SQL drivers",
)
class RefusedRowPointerType:
    pass


@refuse(
    surface="sqlite3.Connection.pragma_query",
    concept="concept:sql-pragma",
    reason="PRAGMA bindings deferred to provekit-shim-python-sqlite3 v0.2. The Python sqlite3 PRAGMA surface is via raw connection.execute('PRAGMA key = value') which returns a Cursor, not a typed result. The API shape differs from rusqlite's typed PRAGMA methods enough that substrate-honest binding requires API-shape verification first.",
    would_close_with_cluster="Verified API shape for Python sqlite3 PRAGMA family",
)
class RefusedPragmaQuery:
    pass


@refuse(
    surface="sqlite3.Connection.pragma_update",
    concept="concept:sql-pragma",
    reason="Same as refused_pragma_query: API shape verification pending for v0.2. Python sqlite3 exposes PRAGMAs only through execute('PRAGMA ...') returning Cursor; there is no typed PRAGMA setter. Refusing rather than shipping a shape that is not API-verified.",
    would_close_with_cluster="Verified API shape for Python sqlite3 PRAGMA family",
)
class RefusedPragmaUpdate:
    pass


@refuse(
    surface="sqlite3.Connection.db_name",
    concept="concept:contract-observation",
    reason="Python sqlite3 has no equivalent of rusqlite Connection.db_name(). The Python stdlib module does not expose attached database names via a method; they are only accessible through 'PRAGMA database_list'. Deferred: needs API shape verification for PRAGMA-based enumeration to close this observation concept.",
    would_close_with_cluster="Verified API for attached-database name enumeration on >=2 Python SQLite drivers",
)
class RefusedDbName:
    pass


@refuse(
    surface="sqlite3.Connection.interrupt",
    concept="concept:sql-connection-interrupt",
    reason="sqlite3.Connection.interrupt() cancels any pending database operation. rusqlite has Connection.interrupt() too, but the Rust shim did not bind it. N=1 is the current state: only one kit (Python) would bind this concept. Refusing to mint a single-kit concept hub; close when rusqlite or another kit joins.",
    would_close_with_cluster="interrupt() cancellation method on >=2 SQL drivers",
)
class RefusedInterrupt:
    pass
