package com.provekit.emit.junit;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.Optional;

import org.junit.jupiter.api.Test;

import com.provekit.ir.Jcs;

/**
 * Per-predicate unit tests for the inline predicate -> JUnit5 assertion
 * mapping. One positive case per supported predicate head, plus a
 * discrimination case (unsupported head refuses) and a structural case
 * (wrong arity refuses).
 */
class PredicateAssertionTableTest {

    private static Jcs.Obj var(String name) {
        return (Jcs.Obj) Jcs.parse(
            "{\"kind\":\"var\",\"name\":\"" + name + "\"}");
    }

    private static Jcs.Obj intConst(long v) {
        return (Jcs.Obj) Jcs.parse(
            "{\"kind\":\"const\",\"value\":" + v
            + ",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}");
    }

    private static Jcs.Obj binary(String concept, Jcs.Json a, Jcs.Json b) {
        return (Jcs.Obj) Jcs.parse(
            "{\"kind\":\"op\",\"name\":\"concept:" + concept + "\",\"args\":["
            + Jcs.encode(a) + "," + Jcs.encode(b) + "]}");
    }

    private static Jcs.Obj unary(String concept, Jcs.Json a) {
        return (Jcs.Obj) Jcs.parse(
            "{\"kind\":\"op\",\"name\":\"concept:" + concept + "\",\"args\":["
            + Jcs.encode(a) + "]}");
    }

    private static String render(Jcs.Obj predicate) {
        Optional<String> r = PredicateAssertionTable.render(predicate);
        assertTrue(r.isPresent(), "expected an assertion for " + Jcs.encode(predicate));
        return r.get();
    }

    @Test
    void eqMapsToAssertEquals() {
        assertEquals(
            "assertEquals(a, b);",
            render(binary("eq", var("a"), var("b"))));
    }

    @Test
    void neMapsToAssertNotEquals() {
        assertEquals(
            "assertNotEquals(a, b);",
            render(binary("ne", var("a"), var("b"))));
    }

    @Test
    void neqAliasAlsoMapsToAssertNotEquals() {
        assertEquals(
            "assertNotEquals(a, b);",
            render(binary("neq", var("a"), var("b"))));
    }

    @Test
    void ltMapsToAssertTrueLess() {
        assertEquals(
            "assertTrue(a < b);",
            render(binary("lt", var("a"), var("b"))));
    }

    @Test
    void gtMapsToAssertTrueGreater() {
        assertEquals(
            "assertTrue(a > b);",
            render(binary("gt", var("a"), var("b"))));
    }

    @Test
    void leMapsToAssertTrueLessEqual() {
        assertEquals(
            "assertTrue(a <= b);",
            render(binary("le", var("a"), var("b"))));
    }

    @Test
    void geMapsToAssertTrueGreaterEqual() {
        assertEquals(
            "assertTrue(a >= b);",
            render(binary("ge", var("a"), var("b"))));
    }

    @Test
    void optionIsSomeMapsToAssertNotNull() {
        assertEquals(
            "assertNotNull(x);",
            render(unary("option-is-some", var("x"))));
    }

    @Test
    void optionIsNoneMapsToAssertNull() {
        assertEquals(
            "assertNull(x);",
            render(unary("option-is-none", var("x"))));
    }

    @Test
    void fallibleErrMapsToAssertThrows() {
        assertEquals(
            "assertThrows(Exception.class, () -> { parse(s); });",
            render(unary("fallible-err",
                Jcs.parse("{\"kind\":\"op\",\"name\":\"parse\",\"args\":["
                    + Jcs.encode(var("s")) + "]}"))));
    }

    @Test
    void constantOperandRendersLiteral() {
        assertEquals(
            "assertTrue(x >= 0);",
            render(binary("ge", var("x"), intConst(0))));
    }

    @Test
    void arithmeticOperandRendersInfix() {
        Jcs.Json sum = Jcs.parse(
            "{\"kind\":\"ctor\",\"name\":\"+\",\"args\":["
            + Jcs.encode(var("a")) + "," + Jcs.encode(var("b")) + "]}");
        assertEquals(
            "assertEquals((a + b), c);",
            render(binary("eq", sum, var("c"))));
    }

    // Discrimination: an unknown predicate head is refused, not silently
    // emitted as a passing assertion.
    @Test
    void unsupportedHeadRefuses() {
        Jcs.Obj p = binary("totally-made-up", var("a"), var("b"));
        assertFalse(PredicateAssertionTable.render(p).isPresent());
        assertFalse(PredicateAssertionTable.supports("totally-made-up"));
    }

    // Structural: wrong arity is refused.
    @Test
    void wrongArityRefuses() {
        Jcs.Obj p = unary("eq", var("a")); // eq needs two args
        assertFalse(PredicateAssertionTable.render(p).isPresent());
    }

    @Test
    void barePlainNameAlsoAccepted() {
        // Harvester internal form: kind "atomic" with bare name (no concept:).
        Jcs.Obj p = (Jcs.Obj) Jcs.parse(
            "{\"kind\":\"atomic\",\"name\":\"eq\",\"args\":["
            + Jcs.encode(var("a")) + "," + Jcs.encode(var("b")) + "]}");
        assertEquals("assertEquals(a, b);", render(p));
    }
}
