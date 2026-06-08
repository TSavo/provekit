# sugar-shim-java-sqlite-jdbc

Sugar substrate-honest sugar bindings for `org.xerial:sqlite-jdbc` (JDBC driver for SQLite).

Sister shim to `sugar-shim-rusqlite` (Rust). Paper 24 §3 cross-library cluster: both kits
declare the same concept names; the substrate recognizes the cluster by structural match on
`term_shape` in each binding's IR.

## Surface model

55 members in the `.proof` envelope:
- 45 `library-sugar-binding-entry` records (concept name, loss dimensions, optional observed_dimension)
- 10 `refusal-memento` records (refused boundaries with reasons)

## Cardinality-split query concepts (#1468)

`concept:sql-query` is split by result cardinality into the global Phase-0
catalog concepts. This shim maps by what each helper actually returns: every
query helper hands back a raw `java.sql.ResultSet` from `executeQuery`, which is
a lazy single-pass cursor the caller walks via `.next()`, so `queryRow`,
`stmtQuery`, `stmtQueryMap`, and `stmtQueryRow` all bind
`concept:sql-query-iterate` (`SqlRowCursor`). No helper materializes a list
(`-all`) or returns a single row or null (`-row`); the `*Row` method names mirror
rusqlite's surface naming but the JDBC helper returns the cursor, not the first
row. `stmtExists` returns a boolean (a cardinality projection) and keeps the flat
`concept:sql-query`; additive coexistence with the pre-split concept is intended.

## Concept alignment with sugar-shim-rusqlite

| Section | Concepts covered |
|---------|-----------------|
| A. Connection lifecycle | `concept:sql-connection-open`, `concept:sql-connection-close` |
| A'. DataSource pooling | `concept:sql-connection-pool-acquire` (N=1 carrier) |
| B. Connection-level execution | `concept:sql-execute`, `concept:sql-query-iterate` |
| C. Preparation | `concept:sql-prepare`, `concept:sql-prepare-cached` |
| D. Statement execution | `concept:sql-execute`, `concept:sql-query-iterate`, `concept:sql-query`, `concept:insert-and-get-id` |
| D'. Batch execute | `concept:sql-batch-execute` (N=1 carrier) |
| E. Transactions | `concept:sql-transaction-begin`, `concept:sql-transaction-commit`, `concept:sql-transaction-rollback`, `concept:sql-savepoint` |
| E'. Isolation level | `concept:sql-transaction-isolation-level` (N=1 carrier) |
| F. Row reading | `concept:sql-row-get-column` |
| G. Changes counting | `concept:insert-and-get-id`, `concept:sql-changes-count` |
| H. State observation | `concept:contract-observation` (autocommit-mode, write-permission, etc.) |
| I. Statement metadata | `concept:contract-observation` (column-count, column-name, etc.) |
| I'. ResultSetMetaData | `concept:sql-result-metadata` (N=1 carrier) |
| I''. DatabaseMetaData | `concept:sql-catalog-reflection` (N=1 carrier) |
| J. Busy timeout | `concept:sql-busy-timeout` |
| J'. Autocommit toggle | `concept:sql-autocommit-control` (N=1 carrier) |
| J''. Fetch size | `concept:sql-cursor-fetch-size` (N=1 carrier) |

## Follow-up gap

The Java realize plugin (`sugar-realize-java`) does not yet support per-library
body-templates routing (routing by `target_library_tag = "sqlite-jdbc"`). The
`java-canonical-bodies-sqlite-jdbc.json` body-templates file (45 entries) is emitted
by `cmd_mint` automatically from the IR. Its `emission_template` values are method-span
verbatim (full annotation + signature + body), not call-expression only as in rusqlite.
No current consumer is affected since the realize plugin gap (#1232) means no routing
occurs yet. The templates will need trimming to call-expression form when the realize
plugin gains per-library routing. This is gated on issue #1232 (mainline realize work).
