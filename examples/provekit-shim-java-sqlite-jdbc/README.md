# provekit-shim-java-sqlite-jdbc

ProvekIt substrate-honest sugar bindings for `org.xerial:sqlite-jdbc` (JDBC driver for SQLite).

Sister shim to `provekit-shim-rusqlite` (Rust). Paper 24 §3 cross-library cluster: both kits
declare the same concept names; the substrate recognizes the cluster by structural match on
`term_shape` in each binding's IR.

## Surface model

54+ members in the `.proof` envelope:
- 44+ `library-sugar-binding-entry` records (concept name, loss dimensions, optional observed_dimension)
- 10 `refusal-memento` records (refused boundaries with reasons)

## Concept alignment with provekit-shim-rusqlite

| Section | Concepts covered |
|---------|-----------------|
| A. Connection lifecycle | `concept:sql-connection-open`, `concept:sql-connection-close` |
| A'. DataSource pooling | `concept:sql-connection-pool-acquire` (N=1 carrier) |
| B. Connection-level execution | `concept:sql-execute`, `concept:sql-query` |
| C. Preparation | `concept:sql-prepare`, `concept:sql-prepare-cached` |
| D. Statement execution | `concept:sql-execute`, `concept:sql-query`, `concept:insert-and-get-id` |
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

The Java realize plugin (`provekit-realize-java`) does not yet support per-library
body-templates routing (routing by `target_library_tag = "sqlite-jdbc"`). The
`java-canonical-bodies-sqlite-jdbc.json` body-templates file is emitted by `cmd_mint`
automatically from the IR, but the realize plugin cannot yet consume it to emit
sqlite-jdbc-specific bodies. This is gated on issue #1232 (mainline realize work).
