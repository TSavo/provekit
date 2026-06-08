package com.sugar.demo.recognize;

import static org.junit.jupiter.api.Assertions.assertEquals;

import java.sql.Connection;
import java.sql.PreparedStatement;
import org.junit.jupiter.api.Test;

class ReportTest {
    @Test
    void loadReadsSqlRowAndDeserializesJsonPayload() throws Exception {
        try (Connection conn = Persist.openConnection("jdbc:sqlite::memory:")) {
            try (PreparedStatement create = conn.prepareStatement("""
                CREATE TABLE events (
                    id INTEGER PRIMARY KEY,
                    type TEXT NOT NULL,
                    user TEXT NOT NULL,
                    payload TEXT NOT NULL
                )
                """)) {
                Persist.executeStatement(create);
            }
            try (PreparedStatement insert = conn.prepareStatement("""
                INSERT INTO events (type, user, payload)
                VALUES ('signup', 'alice', '{"age":30}')
                """)) {
                Persist.executeStatement(insert);
            }

            Report report = Report.load(conn, 1L);

            assertEquals("alice", report.user());
            assertEquals("signup", report.eventType());
            assertEquals(30, report.age());
            assertEquals("{\"age\":30}", report.payloadJson());
        }
    }
}
