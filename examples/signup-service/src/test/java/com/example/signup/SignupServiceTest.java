package com.example.signup;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

/**
 * Ordinary unit tests for the signup service -- the kind anyone writes. To
 * Sugar, each assertion about a library call is a sworn point on that
 * library's contract; the test file is the spec, the dependency's source is
 * the universe, and the two are checked against each other.
 */
class SignupServiceTest {

    private final SignupService service = new SignupService();

    @Test
    void validRequestPasses() {
        SignupRequest req = new SignupRequest("ada", 36, "Ada Lovelace");
        assertTrue(service.isValid(req));
    }

    @Test
    void underageRequestFails() {
        SignupRequest req = new SignupRequest("kid", 9, "Kid");
        assertFalse(service.isValid(req));
    }

    @Test
    void blankUsernameFails() {
        SignupRequest req = new SignupRequest("   ", 30, "Spacey");
        assertFalse(service.isValid(req));
    }

    @Test
    void parsesJsonBody() {
        SignupRequest req = service.parse("{\"username\":\"grace\",\"age\":40,\"displayName\":\"Grace\"}");
        assertEquals("grace", req.getUsername());
        assertEquals(40, req.getAge());
    }

    @Test
    void tokenIsStandardBase64() {
        SignupRequest req = new SignupRequest("ada", 36, "Ada");
        // "ada:36" -> standard base64. This is the vendor-style point sample:
        // a sworn value for a specific codec callsite.
        assertEquals("YWRhOjM2", service.token(req));
    }

    @Test
    void displayNameIsHtmlEscaped() {
        SignupRequest req = new SignupRequest("ada", 36, "<script>");
        assertEquals("&lt;script&gt;", service.safeDisplayName(req));
    }
}
