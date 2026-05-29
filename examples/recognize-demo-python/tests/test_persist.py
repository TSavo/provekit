from recognize_demo_python.persist import connect_db, execute_sql, query_one


def test_connect_db_opens_sqlite_database(tmp_path):
    conn = connect_db(str(tmp_path / "events.sqlite3"))

    try:
        execute_sql(conn, "CREATE TABLE events (id INTEGER PRIMARY KEY, name TEXT NOT NULL)", ())
        execute_sql(conn, "INSERT INTO events (name) VALUES (?)", ("alice",))

        assert query_one(conn, "SELECT name FROM events WHERE id = ?", (1,)) == ("alice",)
    finally:
        conn.close()


def test_query_one_returns_none_when_no_row(tmp_path):
    conn = connect_db(str(tmp_path / "empty.sqlite3"))

    try:
        execute_sql(conn, "CREATE TABLE events (id INTEGER PRIMARY KEY, name TEXT NOT NULL)", ())

        assert query_one(conn, "SELECT name FROM events WHERE id = ?", (99,)) is None
    finally:
        conn.close()
