package com.provekit.realize;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Emission tests for the 7 body-template entries wired in #1158:
 * concept:closure, concept:double-dispatch, concept:dynamic-dispatch,
 * concept:exception, concept:generic-instantiation, concept:iterator,
 * concept:reference.
 *
 * Per feedback_discrimination_tests_per_variant: 3 tests per entry
 * (positive + structural + discrimination). 21 tests total.
 */
public class BodyTemplateWiringTest {

    // -----------------------------------------------------------------------
    // concept:closure (java:lambda-invokedynamic)
    // -----------------------------------------------------------------------

    @Test
    public void closureBodyTemplateEmitsLambdaExpression() {
        java.util.Optional<String> body = SugarRealizer.bodyTemplateFor(
            "concept:closure",
            java.util.List.of("x", "x * 2")
        );

        assertTrue(body.isPresent(), "concept:closure must resolve a body template");
        assertTrue(body.get().contains("x"), "rendered body must contain bound param");
        assertTrue(body.get().contains("->"), "rendered body must contain lambda arrow");
        assertTrue(body.get().contains("x * 2"), "rendered body must contain bound body expression");
    }

    @Test
    public void closureBodyTemplateHasNoUnboundPlaceholders() {
        java.util.Optional<String> body = SugarRealizer.bodyTemplateFor(
            "concept:closure",
            java.util.List.of("val", "val + 1")
        );

        assertTrue(body.isPresent());
        assertFalse(body.get().contains("${"), "rendered closure must not contain unbound placeholders");
        assertFalse(body.get().isEmpty(), "rendered closure must not be empty");
    }

    @Test
    public void closureBodyTemplateRejectedForWrongConceptName() {
        java.util.Optional<String> body = SugarRealizer.bodyTemplateFor(
            "concept:function",
            java.util.List.of("x", "x + 1")
        );

        assertTrue(body.isEmpty(),
            "concept:function must not match the concept:closure entry");
    }

    // -----------------------------------------------------------------------
    // concept:double-dispatch (java:visitor-itab-pair)
    // -----------------------------------------------------------------------

    @Test
    public void doubleDispatchBodyTemplateEmitsVisitorAcceptPair() {
        java.util.Optional<String> body = SugarRealizer.bodyTemplateFor(
            "concept:double-dispatch",
            java.util.List.of("visitor", "visitee")
        );

        assertTrue(body.isPresent(), "concept:double-dispatch must resolve a body template");
        assertTrue(body.get().contains("visitor"), "rendered body must reference visitor param");
        assertTrue(body.get().contains("visitee"), "rendered body must reference visitee param");
        assertTrue(body.get().contains("accept"), "visitor pattern body must contain accept call");
    }

    @Test
    public void doubleDispatchBodyTemplateHasNoUnboundPlaceholders() {
        java.util.Optional<String> body = SugarRealizer.bodyTemplateFor(
            "concept:double-dispatch",
            java.util.List.of("v1", "v2")
        );

        assertTrue(body.isPresent());
        assertFalse(body.get().contains("${"),
            "rendered double-dispatch must not contain unbound placeholders");
        assertFalse(body.get().isEmpty());
    }

    @Test
    public void doubleDispatchBodyTemplateRejectedForWrongParamCount() {
        java.util.Optional<String> body = SugarRealizer.bodyTemplateFor(
            "concept:double-dispatch",
            java.util.List.of("visitor")
        );

        assertTrue(body.isEmpty(),
            "concept:double-dispatch requires 2 params; 1 must return empty");
    }

    // -----------------------------------------------------------------------
    // concept:dynamic-dispatch (java:virtual-method)
    // -----------------------------------------------------------------------

    @Test
    public void dynamicDispatchBodyTemplateEmitsVirtualCall() {
        java.util.Optional<String> body = SugarRealizer.bodyTemplateFor(
            "concept:dynamic-dispatch",
            java.util.List.of("receiver", "process", "arg")
        );

        assertTrue(body.isPresent(), "concept:dynamic-dispatch must resolve a body template");
        assertTrue(body.get().contains("receiver"), "rendered body must contain receiver");
        assertTrue(body.get().contains("process"), "rendered body must contain method name");
        assertTrue(body.get().contains("arg"), "rendered body must contain args");
        assertTrue(body.get().contains("return"), "virtual call must emit a return statement");
    }

    @Test
    public void dynamicDispatchBodyTemplateHasNoUnboundPlaceholders() {
        java.util.Optional<String> body = SugarRealizer.bodyTemplateFor(
            "concept:dynamic-dispatch",
            java.util.List.of("obj", "run", "input")
        );

        assertTrue(body.isPresent());
        assertFalse(body.get().contains("${"),
            "rendered dynamic-dispatch must not contain unbound placeholders");
        assertFalse(body.get().isEmpty());
    }

    @Test
    public void dynamicDispatchBodyTemplateRejectedForWrongConceptName() {
        java.util.Optional<String> body = SugarRealizer.bodyTemplateFor(
            "concept:static-dispatch",
            java.util.List.of("receiver", "method", "arg")
        );

        assertTrue(body.isEmpty(),
            "concept:static-dispatch must not match the concept:dynamic-dispatch entry");
    }

    // -----------------------------------------------------------------------
    // concept:exception (java:try-catch)
    // -----------------------------------------------------------------------

    @Test
    public void exceptionBodyTemplateEmitsTryCatchBlock() {
        java.util.Optional<String> body = SugarRealizer.bodyTemplateFor(
            "concept:exception",
            java.util.List.of("riskyOp()", "ex", "0L")
        );

        assertTrue(body.isPresent(), "concept:exception must resolve a body template");
        assertTrue(body.get().contains("try"), "exception body must contain try keyword");
        assertTrue(body.get().contains("catch"), "exception body must contain catch keyword");
        assertTrue(body.get().contains("riskyOp()"), "exception body must bind try-block expression");
        assertTrue(body.get().contains("ex"), "exception body must bind exception binding");
        assertTrue(body.get().contains("0L"), "exception body must bind handler expression");
    }

    @Test
    public void exceptionBodyTemplateHasNoUnboundPlaceholders() {
        java.util.Optional<String> body = SugarRealizer.bodyTemplateFor(
            "concept:exception",
            java.util.List.of("compute()", "e", "-1L")
        );

        assertTrue(body.isPresent());
        assertFalse(body.get().contains("${"),
            "rendered exception template must not contain unbound placeholders");
        assertFalse(body.get().isEmpty());
    }

    @Test
    public void exceptionBodyTemplateRejectedForTwoParams() {
        java.util.Optional<String> body = SugarRealizer.bodyTemplateFor(
            "concept:exception",
            java.util.List.of("op()", "e")
        );

        assertTrue(body.isEmpty(),
            "concept:exception requires 3 params; 2 must return empty");
    }

    // -----------------------------------------------------------------------
    // concept:generic-instantiation (java:type-erasure)
    // -----------------------------------------------------------------------

    @Test
    public void genericInstantiationBodyTemplateEmitsErasedCast() {
        java.util.Optional<String> body = SugarRealizer.bodyTemplateFor(
            "concept:generic-instantiation",
            java.util.List.of("rawValue", "String")
        );

        assertTrue(body.isPresent(), "concept:generic-instantiation must resolve a body template");
        assertTrue(body.get().contains("rawValue"), "rendered body must contain the source expression");
        assertTrue(body.get().contains("String"), "rendered body must contain the erased target type");
        assertTrue(body.get().contains("return"), "generic instantiation must emit a return statement");
    }

    @Test
    public void genericInstantiationBodyTemplateHasNoUnboundPlaceholders() {
        java.util.Optional<String> body = SugarRealizer.bodyTemplateFor(
            "concept:generic-instantiation",
            java.util.List.of("obj", "Long")
        );

        assertTrue(body.isPresent());
        assertFalse(body.get().contains("${"),
            "rendered generic-instantiation must not contain unbound placeholders");
        assertFalse(body.get().isEmpty());
    }

    @Test
    public void genericInstantiationBodyTemplateRejectedForWrongConceptName() {
        java.util.Optional<String> body = SugarRealizer.bodyTemplateFor(
            "concept:specialization",
            java.util.List.of("obj", "Long")
        );

        assertTrue(body.isEmpty(),
            "concept:specialization must not match the concept:generic-instantiation entry");
    }

    // -----------------------------------------------------------------------
    // concept:iterator (java:iterable-iterator)
    // -----------------------------------------------------------------------

    @Test
    public void iteratorBodyTemplateEmitsEnhancedForLoop() {
        java.util.Optional<String> body = SugarRealizer.bodyTemplateFor(
            "concept:iterator",
            java.util.List.of("items")
        );

        assertTrue(body.isPresent(), "concept:iterator must resolve a body template");
        assertTrue(body.get().contains("items"), "rendered body must reference the iterable param");
        assertTrue(body.get().contains("for"), "iterator body must contain a for loop");
        assertTrue(body.get().contains("return"), "iterator body must contain a return statement");
    }

    @Test
    public void iteratorBodyTemplateHasNoUnboundPlaceholders() {
        java.util.Optional<String> body = SugarRealizer.bodyTemplateFor(
            "concept:iterator",
            java.util.List.of("collection")
        );

        assertTrue(body.isPresent());
        assertFalse(body.get().contains("${"),
            "rendered iterator template must not contain unbound placeholders");
        assertFalse(body.get().isEmpty());
    }

    @Test
    public void iteratorBodyTemplateRejectedForWrongConceptName() {
        java.util.Optional<String> body = SugarRealizer.bodyTemplateFor(
            "concept:loop",
            java.util.List.of("items")
        );

        assertTrue(body.isEmpty(),
            "concept:loop must not match the concept:iterator entry");
    }

    // -----------------------------------------------------------------------
    // concept:reference (java:object-reference)
    // -----------------------------------------------------------------------

    @Test
    public void referenceBodyTemplateEmitsPassThroughReturn() {
        java.util.Optional<String> body = SugarRealizer.bodyTemplateFor(
            "concept:reference",
            java.util.List.of("target")
        );

        assertTrue(body.isPresent(), "concept:reference must resolve a body template");
        assertTrue(body.get().contains("target"), "rendered body must contain the reference param");
        assertTrue(body.get().contains("return"), "reference body must emit a return statement");
    }

    @Test
    public void referenceBodyTemplateHasNoUnboundPlaceholders() {
        java.util.Optional<String> body = SugarRealizer.bodyTemplateFor(
            "concept:reference",
            java.util.List.of("val")
        );

        assertTrue(body.isPresent());
        assertFalse(body.get().contains("${"),
            "rendered reference template must not contain unbound placeholders");
        assertFalse(body.get().isEmpty());
    }

    @Test
    public void referenceBodyTemplateRejectedForWrongConceptName() {
        java.util.Optional<String> body = SugarRealizer.bodyTemplateFor(
            "concept:pointer",
            java.util.List.of("val")
        );

        assertTrue(body.isEmpty(),
            "concept:pointer must not match the concept:reference entry");
    }
}
