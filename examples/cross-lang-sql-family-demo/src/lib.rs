// Cross-language SQL-family demo for the floating-axes substrate (#1355).
//
// THREE @boundary stubs in rust source, each declaring family=sql with
// library pinned to provekit-shim-rusqlite. When materialized under
// --source-lang rust --target python (or --target typescript / --target
// java), the discovery mode (#1361 chunk 2 part A) resolves each
// boundary to a sister-library manifest in the target language WITHOUT
// changing this source.
//
// Expected discovery report under different targets:
//
//   rust   → python:     2 ambiguous (sql-query / sql-execute match
//                        BOTH python-sqlite3 + python-aiosqlite —
//                        family-floating-library matches multiple
//                        sisters) + 1 resolve (sql-connection-open
//                        → python-sqlite3 unambiguously, aiosqlite
//                        doesn't declare it).
//   rust   → typescript: 2 ambiguous (better-sqlite3 + pg both declare
//                        query / execute) + 1 refuse (sql-connection-open
//                        is in neither's provides_concepts).
//   rust   → java:       3 resolve (sqlite-jdbc covers all three).
//
// The point: the SAME rust source resolves to its sister-library
// implementation in 3 other languages with no code changes. The
// substrate matches by family + concept, not by language-specific
// library name. This is the four-axis floating-axes dispatch
// (language × family × library × version × concept) working end-to-end.

#[provekit::boundary(
    concept = "concept:sql-query",
    library = "provekit-shim-rusqlite",
    family = "concept:family:sql",
    version = "0.39",
    boundary_contract = "boundary:sql-query",
)]
pub fn run_query(conn: &i64, sql: &str) -> i64 {
    unimplemented!()
}

#[provekit::boundary(
    concept = "concept:sql-execute",
    library = "provekit-shim-rusqlite",
    family = "concept:family:sql",
    version = "0.39",
    boundary_contract = "boundary:sql-execute",
)]
pub fn run_execute(conn: &i64, sql: &str) -> i64 {
    unimplemented!()
}

#[provekit::boundary(
    concept = "concept:sql-connection-open",
    library = "provekit-shim-rusqlite",
    family = "concept:family:sql",
    version = "0.39",
    boundary_contract = "boundary:sql-connection-open",
)]
pub fn run_open(path: &str) -> i64 {
    unimplemented!()
}
