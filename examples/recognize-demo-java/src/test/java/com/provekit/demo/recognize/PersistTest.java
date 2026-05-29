package com.provekit.demo.recognize;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.sql.Connection;
import java.sql.DriverManager;
import java.sql.PreparedStatement;
import java.sql.ResultSet;
import org.junit.jupiter.api.Test;

class PersistTest {
    @Test
    void openConnectionOpensSqliteJdbcUrl() throws Exception {
        try (Connection conn = Persist.openConnection("jdbc:sqlite::memory:")) {
            assertFalse(conn.isClosed());
        }
    }

    @Test
    void executeStatementRunsDdlAndDml() throws Exception {
        try (Connection conn = DriverManager.getConnection("jdbc:sqlite::memory:")) {
            try (PreparedStatement create = conn.prepareStatement(
                    "CREATE TABLE events (id INTEGER PRIMARY KEY, user TEXT NOT NULL)")) {
                Persist.executeStatement(create);
            }

            int changed;
            try (PreparedStatement insert = conn.prepareStatement("INSERT INTO events (user) VALUES ('alice')")) {
                changed = Persist.executeStatement(insert);
            }

            assertEquals(1, changed);
        }
    }

    @Test
    void queryRowsReturnsCursorForStatementResults() throws Exception {
        try (Connection conn = DriverManager.getConnection("jdbc:sqlite::memory:")) {
            try (PreparedStatement create = conn.prepareStatement(
                    "CREATE TABLE events (id INTEGER PRIMARY KEY, user TEXT NOT NULL)")) {
                Persist.executeStatement(create);
            }
            try (PreparedStatement insert = conn.prepareStatement("INSERT INTO events (user) VALUES ('alice')")) {
                Persist.executeStatement(insert);
            }

            try (ResultSet rows = Persist.queryRows(conn, "SELECT user FROM events")) {
                assertTrue(rows.next());
                assertEquals("alice", rows.getString(1));
                assertFalse(rows.next());
            }
        }
    }
}
