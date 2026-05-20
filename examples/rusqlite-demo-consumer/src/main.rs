// SPDX-License-Identifier: Apache-2.0
//
// Pre-materialize downstream consumer. Each function declares, via a
// concept-citation carrier comment, which concept its body realizes
// and which library_tag should fill it. The bodies are `unimplemented!()`
// stubs.
//
// `provekit materialize --library rust-rusqlite --source-dir src
// --project .` rewrites these stubs through the shim's published
// .proof envelope (paper 24 §5: the seam is the trade). Each
// rewrite is a TradeMemento between this consumer's signed
// concept-citation and the shim's signed binding under the rusqlite
// library_tag.
//
// Until materialize runs, the program does not work. That is the
// point: the substrate is the only thing that can fill the stubs,
// and only through a signed boundary join at the cited concept hub
// CIDs.

use rusqlite::{Connection, Result, Statement, Transaction};

// provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:sql-connection-open","function":"open_in_memory","params":[],"param_types":[],"return_type":"rusqlite::Result<rusqlite::Connection>","library_tag":"rusqlite"}
fn open_in_memory() -> Result<Connection> {
    unimplemented!("provekit materialize fills this from the shim's .proof envelope")
}

// provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:sql-execute","function":"create_users_table","params":["conn"],"param_types":["&rusqlite::Connection"],"return_type":"rusqlite::Result<usize>","library_tag":"rusqlite"}
fn create_users_table(_conn: &Connection) -> Result<usize> {
    unimplemented!("provekit materialize fills this from the shim's .proof envelope")
}

// provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:sql-execute","function":"insert_user","params":["conn","name","email"],"param_types":["&rusqlite::Connection","&str","&str"],"return_type":"rusqlite::Result<usize>","library_tag":"rusqlite"}
fn insert_user(_conn: &Connection, _name: &str, _email: &str) -> Result<usize> {
    unimplemented!("provekit materialize fills this from the shim's .proof envelope")
}

// provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:insert-and-get-id","function":"last_insert_rowid","params":["conn"],"param_types":["&rusqlite::Connection"],"return_type":"i64","library_tag":"rusqlite"}
fn last_insert_rowid(_conn: &Connection) -> i64 {
    unimplemented!("provekit materialize fills this from the shim's .proof envelope")
}

// provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:sql-query","function":"get_user_email","params":["conn","user_id"],"param_types":["&rusqlite::Connection","i64"],"return_type":"rusqlite::Result<String>","library_tag":"rusqlite"}
fn get_user_email(_conn: &Connection, _user_id: i64) -> Result<String> {
    unimplemented!("provekit materialize fills this from the shim's .proof envelope")
}

// provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:sql-prepare","function":"prepare_list","params":["conn"],"param_types":["&rusqlite::Connection"],"return_type":"rusqlite::Result<rusqlite::Statement<'_>>","library_tag":"rusqlite"}
fn prepare_list(_conn: &Connection) -> Result<Statement<'_>> {
    unimplemented!("provekit materialize fills this from the shim's .proof envelope")
}

// provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:sql-query","function":"list_users","params":["stmt"],"param_types":["&mut rusqlite::Statement<'_>"],"return_type":"rusqlite::Result<Vec<(i64,String,String)>>","library_tag":"rusqlite"}
fn list_users(_stmt: &mut Statement<'_>) -> Result<Vec<(i64, String, String)>> {
    unimplemented!("provekit materialize fills this from the shim's .proof envelope")
}

// provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:sql-transaction-begin","function":"begin_tx","params":["conn"],"param_types":["&mut rusqlite::Connection"],"return_type":"rusqlite::Result<rusqlite::Transaction<'_>>","library_tag":"rusqlite"}
fn begin_tx(_conn: &mut Connection) -> Result<Transaction<'_>> {
    unimplemented!("provekit materialize fills this from the shim's .proof envelope")
}

// provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:sql-transaction-commit","function":"commit_tx","params":["tx"],"param_types":["rusqlite::Transaction<'_>"],"return_type":"rusqlite::Result<()>","library_tag":"rusqlite"}
fn commit_tx(_tx: Transaction<'_>) -> Result<()> {
    unimplemented!("provekit materialize fills this from the shim's .proof envelope")
}

// provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:sql-query","function":"count_users","params":["conn"],"param_types":["&rusqlite::Connection"],"return_type":"rusqlite::Result<i64>","library_tag":"rusqlite"}
fn count_users(_conn: &Connection) -> Result<i64> {
    unimplemented!("provekit materialize fills this from the shim's .proof envelope")
}

fn main() -> Result<()> {
    let mut conn = open_in_memory()?;
    create_users_table(&conn)?;
    insert_user(&conn, "Alice", "alice@example.com")?;
    let alice_id = last_insert_rowid(&conn);
    insert_user(&conn, "Bob", "bob@example.com")?;
    let alice_email = get_user_email(&conn, alice_id)?;
    println!("inserted Alice with id={alice_id}, email={alice_email}");

    let mut stmt = prepare_list(&conn)?;
    let users = list_users(&mut stmt)?;
    println!("\nall users:");
    for (id, name, email) in &users {
        println!("  {id}: {name} <{email}>");
    }

    let tx = begin_tx(&mut conn)?;
    // Note: this insert is in the transaction context; the carrier
    // declares the concept for the txn-scoped execute.
    // provekit-concept: {"artifact_kind":"provekit-concept-citation-comment-sugar","concept_name":"concept:sql-execute","function":"insert_user_in_tx","params":["tx","name","email"],"param_types":["&rusqlite::Transaction<'_>","&str","&str"],"return_type":"rusqlite::Result<usize>","library_tag":"rusqlite"}
    fn insert_user_in_tx(_tx: &Transaction<'_>, _name: &str, _email: &str) -> Result<usize> {
        unimplemented!("provekit materialize fills this from the shim's .proof envelope")
    }
    insert_user_in_tx(&tx, "Charlie", "charlie@example.com")?;
    commit_tx(tx)?;

    let count = count_users(&conn)?;
    println!("\nfinal count: {count}");

    Ok(())
}
