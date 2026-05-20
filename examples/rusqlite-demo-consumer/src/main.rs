// SPDX-License-Identifier: Apache-2.0
//
// Pre-materialize downstream consumer. Each stub function's signature
// matches the corresponding sugar binding in `provekit-shim-rusqlite`'s
// published .proof envelope exactly: same arity, same positional
// parameter shape. The bodies are `unimplemented!()` placeholders that
// `provekit materialize --library rust-rusqlite --source-dir src
// --project .` rewrites through the shim's signed bindings.
//
// Per paper 24 §5, the carrier-comment seam IS the trade: each
// citation pairs the consumer's signed concept claim with the kit's
// signed realization claim under the rust-rusqlite library_tag. The
// substrate verifies that the consumer's signature shape matches the
// binding's signature shape; if it does not, the realize plugin
// returns a stub and materialize refuses (substrate-honest).
//
// `main()` chains the stubs and passes SQL strings inline, because
// `concept:sql-execute` and `concept:sql-query` are SQL-string-taking
// primitives. The carrier-comment payload does not carry the SQL
// fragment; it carries only the concept identity. The downstream
// consumer's own SQL is its own concern.

use rusqlite::{Connection, Params, Result, Row, Statement, Transaction};

// provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:sql-connection-open","function":"open_in_memory","params":[],"param_types":[],"return_type":"rusqlite::Result<rusqlite::Connection>","library_tag":"rusqlite"}
fn open_in_memory() -> Result<Connection> {
    unimplemented!("provekit materialize fills this from the shim's .proof envelope")
}

// provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:sql-execute","function":"execute","params":["conn","sql","params"],"param_types":["&rusqlite::Connection","&str","P"],"return_type":"rusqlite::Result<usize>","library_tag":"rusqlite"}
fn execute<P: Params>(_conn: &Connection, _sql: &str, _params: P) -> Result<usize> {
    unimplemented!("provekit materialize fills this from the shim's .proof envelope")
}

// provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:sql-query","function":"query_row","params":["conn","sql","params","mapper"],"param_types":["&rusqlite::Connection","&str","P","F"],"return_type":"rusqlite::Result<T>","library_tag":"rusqlite"}
fn query_row<T, P: Params, F: FnOnce(&Row<'_>) -> Result<T>>(
    _conn: &Connection,
    _sql: &str,
    _params: P,
    _mapper: F,
) -> Result<T> {
    unimplemented!("provekit materialize fills this from the shim's .proof envelope")
}

// provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:sql-prepare","function":"prepare","params":["conn","sql"],"param_types":["&rusqlite::Connection","&str"],"return_type":"rusqlite::Result<rusqlite::Statement<'_>>","library_tag":"rusqlite"}
fn prepare<'conn>(_conn: &'conn Connection, _sql: &str) -> Result<Statement<'conn>> {
    unimplemented!("provekit materialize fills this from the shim's .proof envelope")
}

// provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:sql-query","function":"stmt_query_map","params":["stmt","params","mapper"],"param_types":["&mut rusqlite::Statement<'_>","P","F"],"return_type":"rusqlite::Result<rusqlite::MappedRows<'_,F>>","library_tag":"rusqlite"}
fn stmt_query_map<'stmt, T, P, F>(
    _stmt: &'stmt mut Statement<'_>,
    _params: P,
    _mapper: F,
) -> Result<rusqlite::MappedRows<'stmt, F>>
where
    P: Params,
    F: FnMut(&Row<'_>) -> Result<T>,
{
    unimplemented!("provekit materialize fills this from the shim's .proof envelope")
}

// provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:insert-and-get-id","function":"last_insert_rowid","params":["conn"],"param_types":["&rusqlite::Connection"],"return_type":"i64","library_tag":"rusqlite"}
fn last_insert_rowid(_conn: &Connection) -> i64 {
    unimplemented!("provekit materialize fills this from the shim's .proof envelope")
}

// provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:sql-transaction-begin","function":"transaction","params":["conn"],"param_types":["&mut rusqlite::Connection"],"return_type":"rusqlite::Result<rusqlite::Transaction<'_>>","library_tag":"rusqlite"}
fn transaction<'conn>(_conn: &'conn mut Connection) -> Result<Transaction<'conn>> {
    unimplemented!("provekit materialize fills this from the shim's .proof envelope")
}

// provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:sql-transaction-commit","function":"tx_commit","params":["tx"],"param_types":["rusqlite::Transaction<'_>"],"return_type":"rusqlite::Result<()>","library_tag":"rusqlite"}
fn tx_commit(_tx: Transaction<'_>) -> Result<()> {
    unimplemented!("provekit materialize fills this from the shim's .proof envelope")
}

fn main() -> Result<()> {
    let mut conn = open_in_memory()?;

    execute(
        &conn,
        "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL, email TEXT NOT NULL UNIQUE)",
        [],
    )?;

    execute(
        &conn,
        "INSERT INTO users (name, email) VALUES (?1, ?2)",
        rusqlite::params!["Alice", "alice@example.com"],
    )?;
    let alice_id = last_insert_rowid(&conn);

    execute(
        &conn,
        "INSERT INTO users (name, email) VALUES (?1, ?2)",
        rusqlite::params!["Bob", "bob@example.com"],
    )?;

    let alice_email: String = query_row(
        &conn,
        "SELECT email FROM users WHERE id = ?1",
        rusqlite::params![alice_id],
        |row| row.get(0),
    )?;
    println!("inserted Alice with id={alice_id}, email={alice_email}");

    {
        let mut stmt = prepare(&conn, "SELECT id, name, email FROM users ORDER BY id")?;
        let users: Vec<(i64, String, String)> =
            stmt_query_map(&mut stmt, [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
                .collect::<Result<Vec<_>>>()?;
        println!("\nall users:");
        for (id, name, email) in &users {
            println!("  {id}: {name} <{email}>");
        }
    }

    {
        let tx = transaction(&mut conn)?;
        execute(
            &tx,
            "INSERT INTO users (name, email) VALUES (?1, ?2)",
            rusqlite::params!["Charlie", "charlie@example.com"],
        )?;
        tx_commit(tx)?;
    }

    let count: i64 = query_row(&conn, "SELECT COUNT(*) FROM users", [], |row| row.get(0))?;
    println!("\nfinal count: {count}");

    Ok(())
}
