package com.provekit.demo.recognize;

import java.sql.Connection;
import java.sql.PreparedStatement;
import java.sql.ResultSet;

public final class Main {
    private Main() {
    }

    public static void main(String[] args) throws Exception {
        System.out.println(run());
    }

    public static String run() throws Exception {
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

            long rowid;
            try (ResultSet rows = Persist.queryRows(conn, "SELECT last_insert_rowid()")) {
                rows.next();
                rowid = rows.getLong(1);
            }

            Report report = Report.load(conn, rowid);
            return "recognize-demo-java: rowid=" + rowid
                + " user=" + report.user()
                + " type=" + report.eventType()
                + " age=" + report.age()
                + " payload=" + report.payloadJson();
        }
    }
}
