package com.provekit.lift;

import static org.junit.jupiter.api.Assertions.*;

import org.junit.jupiter.api.Test;

public class JavaProductionWalkIntegrationTest {
    @Test
    public void liftShowsProductionComposesButJUnitContractsConflict() {
        String source = """
            import org.junit.jupiter.api.Test;
            import static org.junit.jupiter.api.Assertions.*;

            public class App {
                static int checked(int x) {
                    if (x < 10) {
                        throw new IllegalArgumentException("x must be >= 10");
                    }
                    return x;
                }

                static int composedOk() {
                    int y = 42;
                    return checked(y);
                }

                @Test
                void checkedReturns42() {
                    int actual = checked(42);
                    assertEquals(42, actual);
                }

                @Test
                void checkedDoesNotReturn42() {
                    int actual = checked(42);
                    assertNotEquals(42, actual);
                }
            }
            """;

        String response = new LiftHandler().parseSource("/tmp/App.java", source);

        assertTrue(response.contains("\"implications\":["), response);
        assertTrue(response.contains("\"prover\":\"java-wp-walk\""), response);
        assertTrue(response.contains("\"symbol\":\"checked@App.java:"), response);
        assertTrue(response.contains("::callsite\""), response);
        assertTrue(response.contains("::let:y\""), response);
        assertTrue(response.contains("::entry\""), response);
        assertTrue(response.contains("\"precondition\":{\"kind\":\"atomic\",\"name\":\"gte\",\"args\":["
            + cInt(42)
            + ","
            + cInt(10)
            + "]}"), response);

        assertTrue(response.contains("\"invariant\":{\"kind\":\"atomic\",\"name\":\"eq\""), response);
        assertTrue(response.contains("\"invariant\":{\"kind\":\"atomic\",\"name\":\"neq\""), response);
        assertFalse(response.contains("checkedReturns42"), response);
        assertFalse(response.contains("checkedDoesNotReturn42"), response);
        assertEquals(5, countOccurrences(response, "\"symbol\":\"checked@App.java:"), response);
        assertEquals(3, countOccurrences(response, "\"prover\":\"java-wp-walk\""), response);
    }

    private static String cInt(long value) {
        return "{\"kind\":\"const\",\"value\":" + value
            + ",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}";
    }

    private static int countOccurrences(String haystack, String needle) {
        int count = 0;
        int index = 0;
        while ((index = haystack.indexOf(needle, index)) >= 0) {
            count++;
            index += needle.length();
        }
        return count;
    }
}
