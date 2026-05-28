package com.provekit.demo.recognize;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import java.io.IOException;
import java.sql.Connection;
import java.sql.ResultSet;
import java.sql.SQLException;

public record Report(String user, String eventType, int age, String payloadJson) {
    private static final ObjectMapper MAPPER = new ObjectMapper();

    public static Report load(Connection conn, long rowid) throws SQLException, IOException {
        try (ResultSet rows = Persist.queryRows(
                conn,
                "SELECT user, type, payload FROM events WHERE id = " + rowid)) {
            if (!rows.next()) {
                throw new SQLException("event row not found: " + rowid);
            }
            String user = rows.getString(1);
            String eventType = rows.getString(2);
            String payloadText = rows.getString(3);
            JsonNode payload = MAPPER.readTree(payloadText);
            return new Report(user, eventType, payload.path("age").asInt(), payload.toString());
        }
    }
}
