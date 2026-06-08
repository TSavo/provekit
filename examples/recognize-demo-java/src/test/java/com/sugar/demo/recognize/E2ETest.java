package com.sugar.demo.recognize;

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;

class E2ETest {
    @Test
    void mainRunExecutesFullSqlJsonRoundTrip() throws Exception {
        String output = Main.run();

        assertEquals(
            "recognize-demo-java: rowid=1 user=alice type=signup age=30 payload={\"age\":30}",
            output
        );
    }
}
