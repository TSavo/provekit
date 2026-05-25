// SPDX-License-Identifier: Apache-2.0
//
// provekit-shim-java-sqlite-jdbc: substrate-honest concept bindings for the
// org.xerial:sqlite-jdbc Java JDBC driver for SQLite.
//
// Sister shim to provekit-shim-rusqlite (Rust). Paper 24 §3 cross-library
// cluster: this kit and the rusqlite kit declare the same concept names;
// the substrate recognizes the cluster by structural match on term_shape
// carried in each binding's IR. There is no central concept-spec authoring
// step: the cluster IS the concept (paper 24 §2, paper 21 §6 Authored path).
//
// Every claim this kit makes is in this file. There are no sidecar files.
// The substrate-uniform pattern is: the JavaBindLifter reads this source,
// extracts the structural shape of each @ProveKitSugar-annotated method body,
// attaches the per-binding loss declarations from the annotation arguments,
// attaches the observedDimension for observation bindings, and emits
// refusal-memento IR for each @ProveKitRefuse-annotated inner class.
// cmd_mint consumes the lift kit IR over JSON-RPC and produces a signed
// .proof envelope. No format, file path, or declaration in this class
// exists outside what the lift kit reads from this source.
//
// Three speech acts per paper 24:
//   1. @ProveKitSugar(... loss = {})                 materialize (exact)
//   2. @ProveKitSugar(... loss = {"dim1","dim2"})    loudly-bounded-lossy
//   3. @ProveKitRefuse(...)                          refuse with reason
//
// Concept names are vendored under this kit's signature. Other kits joining
// the cluster cite the same names; the substrate recognizes the cluster by
// structural match on the term_shape carried in each binding's IR.
//
// Loss-dimension conventions (paper 24 §3, JDBC vs rusqlite):
//   - "sync-vs-async": sqlite-jdbc is synchronous JDBC; matches rusqlite.
//   - "error-handling-model": JDBC throws checked java.sql.SQLException;
//     rusqlite returns Result<T, rusqlite::Error>. Named here when the
//     difference materially affects the host-language contract shape.
//   - "connection-handle-opacity": java.sql.Connection is an opaque
//     interface handle, matching rusqlite's Connection opacity.
//   - All other rusqlite loss dimensions are preserved verbatim where
//     the concept transfers (auth-mechanism, connection-pooling, etc.).
//   - JDBC-unique dimensions added where the JDBC surface shape introduces
//     new axes not present in rusqlite (e.g., jdbc-checked-exception,
//     statement-handle-type, fetch-size-tuning).

package org.provekit.shim.sqlite_jdbc;

import com.provekit.lift.java_source.ProveKitSugar;
import com.provekit.lift.java_source.ProveKitRefuse;

import java.sql.Connection;
import java.sql.DatabaseMetaData;
import java.sql.DriverManager;
import java.sql.PreparedStatement;
import java.sql.ResultSet;
import java.sql.ResultSetMetaData;
import java.sql.SQLException;
import java.sql.Savepoint;
import java.sql.Statement;
import javax.sql.DataSource;

/**
 * ProvekIt sugar bindings for org.xerial:sqlite-jdbc.
 *
 * All methods delegate directly to the underlying JDBC surface with no
 * business logic. They exist solely so the JavaBindLifter can extract the
 * structural shape and loss declarations and emit them into the proof envelope.
 *
 * Concept alignment: concept names match provekit-shim-rusqlite 1:1 where the
 * surface transfers. JDBC-unique concepts (DataSource pooling, autocommit
 * toggling, fetch-size tuning, ResultSetMetaData, DatabaseMetaData,
 * batch execute, transaction isolation) are introduced as N=1 carriers
 * per the substrate scope rule (N=1 single-kit = carrier, not a hub).
 * They may promote to concept hubs when a second library joins.
 *
 * CARDINALITY-SPLIT QUERY CONCEPTS (#1468)
 * ----------------------------------------
 * A different post-condition is a different contract, so concept:sql-query is
 * split by result cardinality into the global Phase-0 catalog concepts:
 *
 *   - concept:sql-query-row      -> Optional<SqlRow>   (at most one row or null)
 *   - concept:sql-query-all      -> SqlRowSet          (fully-materialized array)
 *   - concept:sql-query-iterate  -> SqlRowCursor       (lazy single-pass cursor)
 *
 * Map by what the bound helper ACTUALLY returns, not by its name. Every query
 * helper in this shim returns a raw java.sql.ResultSet straight from
 * executeQuery, which IS a lazy, single-pass, consume-once cursor whose
 * validity is bound to the cursor lifetime (the caller walks it via .next()).
 * That matches concept:sql-query-iterate's SqlRowCursor post-condition exactly.
 * No helper here materializes a List/array (no -all) or returns Optional<SqlRow>
 * (no -row): the method names queryRow/stmtQueryRow mirror rusqlite's surface
 * naming but, unlike rusqlite::query_row, the JDBC helper hands back the cursor
 * rather than the first row, so binding them to -row would misstate the post.
 * Therefore queryRow, stmtQuery, stmtQueryMap, and stmtQueryRow all bind
 * concept:sql-query-iterate. stmtExists returns a boolean (a cardinality
 * projection, not a row/set/cursor), so it keeps the flat concept:sql-query;
 * additive coexistence with the pre-split concept is intentional.
 */
public final class SqliteJdbcShim {

    private SqliteJdbcShim() {}

    // =========================================================================
    // A. Connection lifecycle
    // Mirrors rusqlite: open/open_in_memory/open_with_flags/close
    // =========================================================================

    @ProveKitSugar(
        concept = "concept:sql-connection-open",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"sync-vs-async", "auth-mechanism", "connection-pooling", "error-handling-model"}
    )
    public static Connection open(String url) throws SQLException {
        return DriverManager.getConnection(url);
    }

    @ProveKitSugar(
        concept = "concept:sql-connection-open",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"sync-vs-async", "persistence-target", "error-handling-model"}
    )
    public static Connection openInMemory() throws SQLException {
        return DriverManager.getConnection("jdbc:sqlite::memory:");
    }

    @ProveKitSugar(
        concept = "concept:sql-connection-open",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"sync-vs-async", "auth-mechanism", "connection-pooling", "error-handling-model",
                "flag-encoding"}
    )
    public static Connection openWithCredentials(String url, String user, String password)
            throws SQLException {
        return DriverManager.getConnection(url, user, password);
    }

    @ProveKitSugar(
        concept = "concept:sql-connection-close",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"sync-vs-async", "error-handling-model"}
    )
    public static void close(Connection conn) throws SQLException {
        conn.close();
    }

    // =========================================================================
    // A'. DataSource connection acquisition (JDBC-unique N=1 carrier)
    // rusqlite has no native DataSource concept; r2d2 is external.
    // concept:sql-connection-pool-acquire is a carrier until a second kit joins.
    // =========================================================================

    @ProveKitSugar(
        concept = "concept:sql-connection-pool-acquire",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"sync-vs-async", "pool-sizing-policy", "error-handling-model",
                "eviction-strategy"}
    )
    public static Connection acquireFromDataSource(DataSource ds) throws SQLException {
        return ds.getConnection();
    }

    // =========================================================================
    // B. Query execution at the Connection level
    // Mirrors rusqlite: execute/execute_batch/query_row/query_row_and_then
    // =========================================================================

    @ProveKitSugar(
        concept = "concept:sql-execute",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"sync-vs-async", "last-insert-id", "transaction-isolation",
                "error-handling-model"}
    )
    public static int execute(Connection conn, String sql) throws SQLException {
        try (Statement stmt = conn.createStatement()) {
            return stmt.executeUpdate(sql);
        }
    }

    @ProveKitSugar(
        concept = "concept:sql-execute",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"sync-vs-async", "error-handling-model", "batch-delimiter-syntax"}
    )
    public static void executeBatch(Connection conn, String sql) throws SQLException {
        try (Statement stmt = conn.createStatement()) {
            stmt.execute(sql);
        }
    }

    @ProveKitSugar(
        concept = "concept:sql-query-iterate",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"sync-vs-async", "row-cardinality", "mapper-side-effects",
                "error-handling-model"}
    )
    public static ResultSet queryRow(Connection conn, String sql) throws SQLException {
        Statement stmt = conn.createStatement();
        return stmt.executeQuery(sql);
    }

    // =========================================================================
    // C. Statement preparation
    // Mirrors rusqlite: prepare/prepare_cached
    // =========================================================================

    @ProveKitSugar(
        concept = "concept:sql-prepare",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"sync-vs-async", "parameter-style", "statement-handle-type",
                "error-handling-model"}
    )
    public static PreparedStatement prepare(Connection conn, String sql) throws SQLException {
        return conn.prepareStatement(sql);
    }

    @ProveKitSugar(
        concept = "concept:sql-prepare-cached",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"sync-vs-async", "cache-eviction-policy", "cache-size-bound",
                "error-handling-model"}
    )
    public static PreparedStatement prepareCached(Connection conn, String sql) throws SQLException {
        // JDBC has no built-in statement cache; delegate to prepareStatement.
        // The loss dimension "cache-eviction-policy" names this gap explicitly.
        return conn.prepareStatement(sql);
    }

    // =========================================================================
    // D. Statement execution
    // Mirrors rusqlite: stmt_execute/stmt_query/stmt_query_map/
    //                   stmt_query_and_then/stmt_query_row/stmt_insert/stmt_exists
    // =========================================================================

    @ProveKitSugar(
        concept = "concept:sql-execute",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"sync-vs-async", "last-insert-id", "transaction-isolation",
                "error-handling-model"}
    )
    public static int stmtExecute(PreparedStatement stmt) throws SQLException {
        return stmt.executeUpdate();
    }

    @ProveKitSugar(
        concept = "concept:sql-query-iterate",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"sync-vs-async", "cursor-lifetime", "error-handling-model"}
    )
    public static ResultSet stmtQuery(PreparedStatement stmt) throws SQLException {
        return stmt.executeQuery();
    }

    @ProveKitSugar(
        concept = "concept:sql-query-iterate",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"sync-vs-async", "cursor-lifetime", "mapper-side-effects",
                "error-handling-model"}
    )
    public static ResultSet stmtQueryMap(PreparedStatement stmt) throws SQLException {
        return stmt.executeQuery();
    }

    @ProveKitSugar(
        concept = "concept:sql-query-iterate",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"sync-vs-async", "row-cardinality", "mapper-side-effects",
                "error-handling-model"}
    )
    public static ResultSet stmtQueryRow(PreparedStatement stmt) throws SQLException {
        return stmt.executeQuery();
    }

    @ProveKitSugar(
        concept = "concept:insert-and-get-id",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"sync-vs-async", "id-column-discovery", "error-handling-model"}
    )
    public static long stmtInsert(PreparedStatement stmt) throws SQLException {
        stmt.executeUpdate();
        try (ResultSet rs = stmt.getGeneratedKeys()) {
            if (rs.next()) return rs.getLong(1);
            return -1L;
        }
    }

    @ProveKitSugar(
        concept = "concept:sql-query",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"sync-vs-async", "cardinality-projected-to-boolean", "error-handling-model"}
    )
    public static boolean stmtExists(PreparedStatement stmt) throws SQLException {
        try (ResultSet rs = stmt.executeQuery()) {
            return rs.next();
        }
    }

    // =========================================================================
    // D'. Parameterized batch execution (JDBC-unique N=1 carrier)
    // rusqlite has no native batch-add/executeBatch; this is JDBC-specific.
    // concept:sql-batch-execute is a carrier.
    // =========================================================================

    @ProveKitSugar(
        concept = "concept:sql-batch-execute",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"sync-vs-async", "error-handling-model", "partial-batch-failure"}
    )
    public static int[] executePreparedBatch(PreparedStatement stmt) throws SQLException {
        return stmt.executeBatch();
    }

    // =========================================================================
    // E. Transaction control
    // Mirrors rusqlite: transaction/transaction_with_behavior/unchecked_transaction/
    //                   tx_commit/tx_rollback/tx_savepoint/tx_set_drop_behavior
    // =========================================================================

    @ProveKitSugar(
        concept = "concept:sql-transaction-begin",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"sync-vs-async", "isolation-level", "nesting-depth-bound",
                "error-handling-model"}
    )
    public static void transactionBegin(Connection conn) throws SQLException {
        conn.setAutoCommit(false);
    }

    @ProveKitSugar(
        concept = "concept:sql-transaction-begin",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"sync-vs-async", "nesting-depth-bound", "error-handling-model",
                "deferred-vs-immediate-vs-exclusive"}
    )
    public static void transactionBeginWithIsolation(Connection conn, int isolationLevel)
            throws SQLException {
        conn.setAutoCommit(false);
        conn.setTransactionIsolation(isolationLevel);
    }

    @ProveKitSugar(
        concept = "concept:sql-transaction-commit",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"sync-vs-async", "two-phase-commit-support", "error-handling-model"}
    )
    public static void txCommit(Connection conn) throws SQLException {
        conn.commit();
    }

    @ProveKitSugar(
        concept = "concept:sql-transaction-rollback",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"sync-vs-async", "partial-rollback-support", "error-handling-model"}
    )
    public static void txRollback(Connection conn) throws SQLException {
        conn.rollback();
    }

    @ProveKitSugar(
        concept = "concept:sql-savepoint",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"nesting-depth-bound", "naming-discipline", "error-handling-model"}
    )
    public static Savepoint txSavepoint(Connection conn) throws SQLException {
        return conn.setSavepoint();
    }

    @ProveKitSugar(
        concept = "concept:sql-transaction-rollback",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"sync-vs-async", "error-handling-model", "savepoint-scope"}
    )
    public static void txRollbackToSavepoint(Connection conn, Savepoint sp)
            throws SQLException {
        conn.rollback(sp);
    }

    // =========================================================================
    // E'. Transaction isolation level control (JDBC-unique N=1 carrier)
    // rusqlite expresses this via TransactionBehavior on begin; JDBC exposes
    // it as a connection-level property independent of begin.
    // concept:sql-transaction-isolation-level is a carrier.
    // =========================================================================

    @ProveKitSugar(
        concept = "concept:sql-transaction-isolation-level",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"sync-vs-async", "error-handling-model", "connection-scope-vs-statement-scope"}
    )
    public static void setTransactionIsolation(Connection conn, int level) throws SQLException {
        conn.setTransactionIsolation(level);
    }

    // =========================================================================
    // F. Row reading
    // Mirrors rusqlite: row_get/row_get_unwrap/row_get_ref
    // =========================================================================

    @ProveKitSugar(
        concept = "concept:sql-row-get-column",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"null-handling", "type-coercion-mode", "error-handling-model"}
    )
    public static Object rowGet(ResultSet rs, int columnIndex) throws SQLException {
        return rs.getObject(columnIndex);
    }

    @ProveKitSugar(
        concept = "concept:sql-row-get-column",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"null-handling", "type-coercion-mode", "error-handling-model",
                "column-name-vs-index"}
    )
    public static Object rowGetByName(ResultSet rs, String columnName) throws SQLException {
        return rs.getObject(columnName);
    }

    @ProveKitSugar(
        concept = "concept:sql-row-get-column",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"null-handling", "type-coercion-mode", "error-handling-model",
                "typed-accessor"}
    )
    public static String rowGetString(ResultSet rs, int columnIndex) throws SQLException {
        return rs.getString(columnIndex);
    }

    @ProveKitSugar(
        concept = "concept:sql-row-get-column",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"null-handling", "type-coercion-mode", "error-handling-model",
                "typed-accessor", "null-unboxing"}
    )
    public static long rowGetLong(ResultSet rs, int columnIndex) throws SQLException {
        return rs.getLong(columnIndex);
    }

    // =========================================================================
    // G. Changes counting + last insert ID
    // Mirrors rusqlite: last_insert_rowid/changes/total_changes
    // =========================================================================

    @ProveKitSugar(
        concept = "concept:insert-and-get-id",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"per-connection-not-per-statement", "rowid-vs-integer-pk",
                "error-handling-model"}
    )
    public static long lastInsertRowId(Connection conn) throws SQLException {
        try (Statement stmt = conn.createStatement();
             ResultSet rs = stmt.executeQuery("SELECT last_insert_rowid()")) {
            return rs.next() ? rs.getLong(1) : -1L;
        }
    }

    @ProveKitSugar(
        concept = "concept:sql-changes-count",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"per-statement-vs-cumulative", "transaction-scope", "error-handling-model"}
    )
    public static int changes(Connection conn) throws SQLException {
        try (Statement stmt = conn.createStatement();
             ResultSet rs = stmt.executeQuery("SELECT changes()")) {
            return rs.next() ? rs.getInt(1) : 0;
        }
    }

    @ProveKitSugar(
        concept = "concept:sql-changes-count",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"cumulative-since-connection-open", "transaction-scope",
                "error-handling-model"}
    )
    public static long totalChanges(Connection conn) throws SQLException {
        try (Statement stmt = conn.createStatement();
             ResultSet rs = stmt.executeQuery("SELECT total_changes()")) {
            return rs.next() ? rs.getLong(1) : 0L;
        }
    }

    // =========================================================================
    // H. Connection state observation
    // Mirrors rusqlite: is_autocommit/is_busy/is_readonly/cache_flush
    // =========================================================================

    @ProveKitSugar(
        concept = "concept:contract-observation",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        observedDimension = "autocommit-mode"
    )
    public static boolean isAutoCommit(Connection conn) throws SQLException {
        return conn.getAutoCommit();
    }

    @ProveKitSugar(
        concept = "concept:contract-observation",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        observedDimension = "write-permission"
    )
    public static boolean isReadOnly(Connection conn) throws SQLException {
        return conn.isReadOnly();
    }

    @ProveKitSugar(
        concept = "concept:contract-observation",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        observedDimension = "connection-closed"
    )
    public static boolean isClosed(Connection conn) throws SQLException {
        return conn.isClosed();
    }

    @ProveKitSugar(
        concept = "concept:contract-observation",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        observedDimension = "connection-validity",
        loss = {"timeout-meaning"}
    )
    public static boolean isValid(Connection conn, int timeout) throws SQLException {
        return conn.isValid(timeout);
    }

    // =========================================================================
    // I. Statement metadata observation
    // Mirrors rusqlite: stmt_column_names/stmt_column_count/
    //                   stmt_column_name/stmt_column_index/
    //                   stmt_expanded_sql/stmt_parameter_count/
    //                   stmt_parameter_name/stmt_parameter_index
    // =========================================================================

    @ProveKitSugar(
        concept = "concept:contract-observation",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        observedDimension = "column-count"
    )
    public static int columnCount(ResultSet rs) throws SQLException {
        return rs.getMetaData().getColumnCount();
    }

    @ProveKitSugar(
        concept = "concept:contract-observation",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        observedDimension = "column-name-at-index"
    )
    public static String columnName(ResultSet rs, int index) throws SQLException {
        return rs.getMetaData().getColumnName(index);
    }

    @ProveKitSugar(
        concept = "concept:contract-observation",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        observedDimension = "column-index-of-name"
    )
    public static int columnIndex(ResultSet rs, String name) throws SQLException {
        return rs.findColumn(name);
    }

    @ProveKitSugar(
        concept = "concept:contract-observation",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        observedDimension = "parameter-count"
    )
    public static int parameterCount(PreparedStatement stmt) throws SQLException {
        return stmt.getParameterMetaData().getParameterCount();
    }

    // =========================================================================
    // I'. ResultSetMetaData reflection (JDBC-unique N=1 carrier)
    // rusqlite exposes column info via Statement methods; JDBC exposes a
    // full ResultSetMetaData object with type info. New concept carrier.
    // =========================================================================

    @ProveKitSugar(
        concept = "concept:sql-result-metadata",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"sync-vs-async", "error-handling-model"}
    )
    public static ResultSetMetaData resultSetMetaData(ResultSet rs) throws SQLException {
        return rs.getMetaData();
    }

    @ProveKitSugar(
        concept = "concept:sql-result-metadata",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        observedDimension = "column-type-name"
    )
    public static String columnTypeName(ResultSetMetaData meta, int index) throws SQLException {
        return meta.getColumnTypeName(index);
    }

    // =========================================================================
    // I''. DatabaseMetaData catalog reflection (JDBC-unique N=1 carrier)
    // rusqlite has no equivalent; JDBC exposes full catalog reflection.
    // concept:sql-catalog-reflection is a carrier.
    // =========================================================================

    @ProveKitSugar(
        concept = "concept:sql-catalog-reflection",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"sync-vs-async", "error-handling-model", "catalog-depth"}
    )
    public static DatabaseMetaData databaseMetaData(Connection conn) throws SQLException {
        return conn.getMetaData();
    }

    // =========================================================================
    // J. Concurrency control
    // Mirrors rusqlite: busy_timeout
    // JDBC uses Statement.setQueryTimeout; concept:sql-busy-timeout aligns.
    // =========================================================================

    @ProveKitSugar(
        concept = "concept:sql-busy-timeout",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"sync-vs-async", "callback-vs-timeout-shape", "timeout-unit",
                "error-handling-model"}
    )
    public static void setQueryTimeout(Statement stmt, int timeoutSeconds)
            throws SQLException {
        stmt.setQueryTimeout(timeoutSeconds);
    }

    // =========================================================================
    // J'. Autocommit toggle (JDBC-unique N=1 carrier)
    // rusqlite manages this implicitly through transaction guards.
    // JDBC exposes it as an explicit connection property.
    // concept:sql-autocommit-control is a carrier.
    // =========================================================================

    @ProveKitSugar(
        concept = "concept:sql-autocommit-control",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"sync-vs-async", "error-handling-model"}
    )
    public static void setAutoCommit(Connection conn, boolean autoCommit)
            throws SQLException {
        conn.setAutoCommit(autoCommit);
    }

    // =========================================================================
    // J''. Fetch size tuning (JDBC-unique N=1 carrier)
    // rusqlite has no fetch-size concept; cursor buffering is automatic.
    // concept:sql-cursor-fetch-size is a carrier.
    // =========================================================================

    @ProveKitSugar(
        concept = "concept:sql-cursor-fetch-size",
        library = "sqlite-jdbc",
        family = "concept:family:sql",
        version = "3.45.3.0",
        loss = {"sync-vs-async", "driver-may-ignore", "error-handling-model"}
    )
    public static void setFetchSize(Statement stmt, int rows) throws SQLException {
        stmt.setFetchSize(rows);
    }

    // =========================================================================
    // Refusals
    // =========================================================================
    //
    // Each refusal is a signed signpost. The substrate publishes the demand
    // the shim declines to fill, naming the cluster constraint that would
    // close it. The JavaBindLifter emits a refusal-memento IR record per
    // annotation; cmd_mint signs each as a RefusalMemento envelope member.
    //
    // Concept names align with provekit-shim-rusqlite 1:1 for cluster
    // coherence. JDBC surface names replace rusqlite surface names.

    @ProveKitRefuse(
        surface = "org.sqlite.SQLiteConnection#backup",
        concept = "concept:sql-physical-backup",
        reason = "org.xerial:sqlite-jdbc does not expose a JDBC-standard physical"
            + " backup method at the connection level. SQLite's online backup API"
            + " is accessible via the SQLiteConnection extension interface, not via"
            + " java.sql.Connection. The Rust side (rusqlite::Connection::backup)"
            + " also refused this concept. Refusing rather than crossing"
            + " standard-JDBC vs extension-API tier boundaries.",
        wouldCloseWithCluster = "Connection-level physical-backup method on >=2 SQL drivers"
    )
    static final class RefusedBackup {}

    @ProveKitRefuse(
        surface = "java.sql.Connection#createBlob",
        concept = "concept:sql-blob-handle",
        reason = "java.sql.Blob provides incremental BLOB I/O, but sqlite-jdbc's"
            + " Blob support is partial: createBlob() is not implemented and throws"
            + " SQLFeatureNotSupportedException. Postgres has lo_open with a"
            + " different lifecycle model. The semantic shapes diverge enough that"
            + " a single-kit cluster would not serve cross-library composition.",
        wouldCloseWithCluster = "Incremental BLOB I/O on >=2 SQL drivers with structurally"
            + " compatible handle semantics"
    )
    static final class RefusedBlobOpen {}

    @ProveKitRefuse(
        surface = "java.sql.Connection#nativeSQL",
        concept = "concept:dynamic-library-load",
        reason = "JDBC's nativeSQL() converts an SQL string to driver-native form."
            + " This is not analogous to OS-level dynamic library loading"
            + " (rusqlite refused load_extension for tier reasons). No concept"
            + " today covers JDBC nativeSQL translation; refusing rather than"
            + " inventing a concept the substrate does not yet carry.",
        wouldCloseWithCluster = "Driver-native SQL translation on >=2 JDBC drivers with"
            + " structurally compatible semantics"
    )
    static final class RefusedLoadExtension {}

    @ProveKitRefuse(
        surface = "java.sql.Connection#setClientInfo",
        concept = "concept:sql-collation-register",
        reason = "JDBC has no concept of custom collation registration at the connection"
            + " level; SQLite custom collations require native callbacks not exposed"
            + " through standard JDBC. Postgres supports CREATE COLLATION SQL;"
            + " the mechanism diverges. Refusing rather than binding a concept"
            + " the standard JDBC surface cannot express.",
        wouldCloseWithCluster = "Custom collation registration on >=2 SQL drivers with"
            + " structurally compatible callback semantics"
    )
    static final class RefusedCreateCollation {}

    @ProveKitRefuse(
        surface = "org.sqlite.SQLiteConfig#setBusyTimeout",
        concept = "concept:sql-busy-handler",
        reason = "JDBC does not expose a callback-based busy handler;"
            + " Statement.setQueryTimeout (which this kit DOES bind as"
            + " concept:sql-busy-timeout) covers the timeout-shaped variant."
            + " SQLite's callback-shaped busy handler requires the native"
            + " org.sqlite extension API, not standard JDBC. Refusing the"
            + " callback-shaped variant.",
        wouldCloseWithCluster = "Callback-based busy-collision handling on >=2 SQL drivers"
    )
    static final class RefusedBusyHandler {}

    @ProveKitRefuse(
        surface = "java.sql.ResultSet#getRef",
        concept = "concept:sql-row-pointer-type",
        reason = "java.sql.Ref is an SQL REF type; sqlite-jdbc throws"
            + " SQLFeatureNotSupportedException for getRef(). SQLite has no"
            + " REF/pointer-column concept. Refusing rather than binding a feature"
            + " that is not implemented in this driver.",
        wouldCloseWithCluster = "Pointer-passing row column type on >=2 SQL drivers"
    )
    static final class RefusedRowGetPointer {}

    @ProveKitRefuse(
        surface = "java.sql.Connection#setCatalog",
        concept = "concept:sql-pragma",
        reason = "SQLite PRAGMA statements have no standard JDBC equivalent."
            + " setCatalog() does not map to PRAGMA semantics. Binding PRAGMA"
            + " via raw Statement.execute(\"PRAGMA ...\") would require verified"
            + " API shapes for all PRAGMA variants before substrate-honest"
            + " declaration. Deferring to v0.2.",
        wouldCloseWithCluster = "PRAGMA binding on >=2 SQLite JDBC drivers with"
            + " verified API shapes"
    )
    static final class RefusedPragmaQuery {}

    @ProveKitRefuse(
        surface = "java.sql.Connection#setCatalog",
        concept = "concept:sql-pragma",
        reason = "Same as RefusedPragmaQuery: PRAGMA_QUERY_VALUE analog absent in"
            + " standard JDBC. Deferring to v0.2 after API shape verification.",
        wouldCloseWithCluster = "PRAGMA binding on >=2 SQLite JDBC drivers with"
            + " verified API shapes"
    )
    static final class RefusedPragmaQueryValue {}

    @ProveKitRefuse(
        surface = "java.sql.Connection#setSchema",
        concept = "concept:sql-pragma",
        reason = "Same as RefusedPragmaQuery: PRAGMA_UPDATE analog absent in"
            + " standard JDBC. Deferring to v0.2 after API shape verification.",
        wouldCloseWithCluster = "PRAGMA update binding on >=2 SQLite JDBC drivers"
    )
    static final class RefusedPragmaUpdate {}

    @ProveKitRefuse(
        surface = "java.sql.Connection#getNetworkTimeout",
        concept = "concept:contract-observation",
        reason = "java.sql.Connection#getNetworkTimeout() is not implemented by"
            + " sqlite-jdbc (throws SQLFeatureNotSupportedException). The"
            + " rusqlite analogue (db_name under modern_sqlite feature) was also"
            + " refused pending cargo-verified confirmation. Deferred to v0.2"
            + " after verifying sqlite-jdbc support.",
        wouldCloseWithCluster = "Network-timeout observation on >=2 JDBC drivers with"
            + " verified implementation"
    )
    static final class RefusedDbName {}
}
