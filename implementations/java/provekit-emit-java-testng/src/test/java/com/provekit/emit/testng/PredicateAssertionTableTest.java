package com.provekit.emit.testng;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.Optional;

import org.junit.jupiter.api.Test;

import com.provekit.ir.Jcs;

/** Per-predicate unit tests for the inline predicate -> TestNG Assert mapping. */
class PredicateAssertionTableTest {

    private static Jcs.Obj var(String name) {
        return (Jcs.Obj) Jcs.parse("{\"kind\":\"var\",\"name\":\"" + name + "\"}");
    }

    private static Jcs.Obj intConst(long v) {
        return (Jcs.Obj) Jcs.parse(
            "{\"kind\":\"const\",\"value\":" + v
            + ",\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}");
    }

    private static Jcs.Obj binary(String concept, Jcs.Json a, Jcs.Json b) {
        return (Jcs.Obj) Jcs.parse(
            "{\"kind\":\"atomic\",\"name\":\"" + concept + "\",\"args\":["
            + Jcs.encode(a) + "," + Jcs.encode(b) + "]}");
    }

    private static Jcs.Obj unary(String concept, Jcs.Json a) {
        return (Jcs.Obj) Jcs.parse(
            "{\"kind\":\"atomic\",\"name\":\"" + concept + "\",\"args\":["
            + Jcs.encode(a) + "]}");
    }

    private static String render(Jcs.Obj predicate) {
        Optional<String> r = PredicateAssertionTable.render(predicate);
        assertTrue(r.isPresent(), "expected an assertion for " + Jcs.encode(predicate));
        return r.get();
    }

    @Test
    void eqMapsToAssertEquals() {
        assertEquals("Assert.assertEquals(a, b);", render(binary("eq", var("a"), var("b"))));
    }

    @Test
    void symbolicEqMapsToAssertEquals() {
        assertEquals("Assert.assertEquals(a, b);", render(binary("=", var("a"), var("b"))));
    }

    @Test
    void neMapsToAssertNotEquals() {
        assertEquals("Assert.assertNotEquals(a, b);", render(binary("ne", var("a"), var("b"))));
    }

    @Test
    void ltMapsToAssertTrueLess() {
        assertEquals("Assert.assertTrue(a < b);", render(binary("lt", var("a"), var("b"))));
    }

    @Test
    void gtMapsToAssertTrueGreater() {
        assertEquals("Assert.assertTrue(a > b);", render(binary("gt", var("a"), var("b"))));
    }

    @Test
    void leMapsToAssertTrueLessEqual() {
        assertEquals("Assert.assertTrue(a <= b);", render(binary("le", var("a"), var("b"))));
    }

    @Test
    void geMapsToAssertTrueGreaterEqual() {
        assertEquals("Assert.assertTrue(a >= b);", render(binary("ge", var("a"), var("b"))));
    }

    @Test
    void optionIsSomeMapsToAssertNotNull() {
        assertEquals("Assert.assertNotNull(x);", render(unary("option-is-some", var("x"))));
    }

    @Test
    void optionIsNoneMapsToAssertNull() {
        assertEquals("Assert.assertNull(x);", render(unary("option-is-none", var("x"))));
    }

    @Test
    void fallibleErrMapsToExpectThrows() {
        assertEquals(
            "Assert.expectThrows(Exception.class, () -> { Object __thrown = parse(s); });",
            render(unary("fallible-err",
                Jcs.parse("{\"kind\":\"op\",\"name\":\"parse\",\"args\":["
                    + Jcs.encode(var("s")) + "]}"))));
    }

    @Test
    void constantOperandRendersLiteral() {
        assertEquals("Assert.assertTrue(x >= 0);", render(binary("ge", var("x"), intConst(0))));
    }

    @Test
    void arithmeticOperandRendersInfix() {
        Jcs.Json sum = Jcs.parse(
            "{\"kind\":\"ctor\",\"name\":\"+\",\"args\":["
            + Jcs.encode(var("a")) + "," + Jcs.encode(var("b")) + "]}");
        assertEquals("Assert.assertEquals((a + b), c);", render(binary("eq", sum, var("c"))));
    }

    @Test
    void conceptPrefixedAtomicNameAlsoAccepted() {
        Jcs.Obj p = (Jcs.Obj) Jcs.parse(
            "{\"kind\":\"atomic\",\"name\":\"concept:eq\",\"args\":["
            + Jcs.encode(var("a")) + "," + Jcs.encode(var("b")) + "]}");
        assertEquals("Assert.assertEquals(a, b);", render(p));
    }

    @Test
    void unsupportedHeadRefuses() {
        Jcs.Obj p = binary("totally-made-up", var("a"), var("b"));
        assertFalse(PredicateAssertionTable.render(p).isPresent());
        assertFalse(PredicateAssertionTable.supports("totally-made-up"));
    }

    @Test
    void wrongArityRefuses() {
        Jcs.Obj p = unary("eq", var("a"));
        assertFalse(PredicateAssertionTable.render(p).isPresent());
    }
}
