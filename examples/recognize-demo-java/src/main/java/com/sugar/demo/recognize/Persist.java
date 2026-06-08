package com.sugar.demo.recognize;

import java.sql.Connection;
import java.sql.DriverManager;
import java.sql.PreparedStatement;
import java.sql.ResultSet;
import java.sql.SQLException;
import java.sql.Statement;

public final class Persist {
    private Persist() {
    }

    public static Connection openConnection(String jdbcUrl) throws SQLException {
        return DriverManager.getConnection(jdbcUrl);
    }

    public static int executeStatement(PreparedStatement stmt) throws SQLException {
        return stmt.executeUpdate();
    }

    public static ResultSet queryRows(Connection conn, String sql) throws SQLException {
        Statement stmt = conn.createStatement();
        return stmt.executeQuery(sql);
    }
}
