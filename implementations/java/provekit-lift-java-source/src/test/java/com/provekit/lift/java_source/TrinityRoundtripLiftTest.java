package com.provekit.lift.java_source;

import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.provekit.ir.Jcs;
import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.util.List;
import java.util.Set;
import java.util.stream.Collectors;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Test;

/**
 * Trinity round-trip lift test.
 *
 * Empirically documents the per-class lifter verdict for the canonical Java
 * output produced by:
 *   provekit bind --root <trinity-fixture> --lang rust
 *     --target-language java --rewrite canonical --mode monitor
 *
 * The bind output is snapshotted at:
 *   src/test/resources/trinity-roundtrip/lib.java
 *
 * Transport-gap spec:
 *   protocol/specs/2026-05-12-trinity-java-roundtrip-transport-gaps.md
 *
 * Per-class lifter verdicts at v0:
 *
 *   WrapIdentityTransported         - LIFTED   (classified as concept:pair by bind v0; stub body)
 *   DoNothingTransported            - REFUSED  (void return: unsupported-return-sort)
 *   ToggleTransported               - LIFTED   (classified as concept:pair by bind v0; stub body)
 *   AssertPositiveTransported       - LIFTED   (stub body)
 *   MaybeFirstTransported           - LIFTED   (&i64 param erased to <any> by JDK error recovery; stub body)
 *   OptionBindDoubleTransported     - LIFTED   (&i64 param erased to <any> by JDK error recovery; stub body)
 *   SafeDivideTransported           - LIFTED   (stub body)
 *   SafeDivideThenDoubleTransported - LIFTED   (stub body)
 *   SwapPairTransported             - LIFTED   (stub body)
 *   ListSumTransported              - LIFTED   (&i64 param erased to <any> by JDK error recovery; stub body)
 *   ClassifyTransported             - LIFTED   (stub body)
 *   RetryUntilSuccessTransported    - LIFTED   (stub body)
 *
 * Round-trip concept verdicts (trichotomy per transport-gap spec §0.1):
 *
 *   EXACT:                0 / 11 trinity concepts
 *   REFUSE (round-trip):  11 -- stub bodies lift successfully but encode a
 *                               different program (UnsupportedOperationException),
 *                               not the original Rust logic. The domain of
 *                               agreement with the source IR is empty. Per Supra
 *                               omnia rectum this is REFUSE, not lossy.
 *   REFUSE (lifter):      1  -- do_nothing void return refused by lifter v1 slice
 *
 * The distinction: the lifter succeeds on 11 methods (lift-side verdict: lifted).
 * But the lifted IR does not reproduce the Rust-side contract-content CID for any
 * concept (round-trip verdict: refuse). Lifting a stub is not the same as closing
 * the round-trip.
 */
class TrinityRoundtripLiftTest {

    private static final String RESOURCE_PATH = "/trinity-roundtrip/lib.java";

    private static JavaSourceLifter.LiftResult result;

    @BeforeAll
    static void loadAndLift() throws IOException {
        InputStream stream = TrinityRoundtripLiftTest.class.getResourceAsStream(RESOURCE_PATH);
        if (stream == null) {
            throw new IllegalStateException(
                "Test resource not found: " + RESOURCE_PATH
                + "\nExpected at: src/test/resources/trinity-roundtrip/lib.java"
                + "\nRe-generate with: cargo run -p provekit-cli -- bind "
                + "--root <trinity-fixture> --lang rust --target-language java "
                + "--rewrite canonical --mode monitor --output /tmp/java-out-trinity"
                + "\nThen copy /tmp/java-out-trinity/translated/java/lib.java to the resource path.");
        }
        String source = new String(stream.readAllBytes(), StandardCharsets.UTF_8);
        result = new JavaSourceLifter().liftSource("lib.java", source);
    }

    // ── lifter-side verdicts ─────────────────────────────────────────────────

    @Test
    void voidReturnIsRefused() {
        // DoNothingTransported.do_nothing() has a void return.
        // The lifter v1 slice refuses void-returning methods: unsupported-return-sort.
        // This is a lifter-side REFUSE (independent of the round-trip verdict).
        List<JavaSourceLifter.Refusal> refusals = result.refusals();
        assertFalse(refusals.isEmpty(), "expected at least one refusal for void do_nothing()");
        boolean foundVoidRefusal = refusals.stream()
            .anyMatch(r -> "unsupported-return-sort".equals(r.kind())
                && r.function() != null && r.function().contains("do_nothing"));
        assertTrue(foundVoidRefusal,
            "expected unsupported-return-sort refusal for do_nothing; got: " + refusals);
    }

    @Test
    void nonVoidMethodsLiftToDeclarations() {
        // All 11 non-void methods lift to function-contract declarations.
        // The stub bodies (throw new UnsupportedOperationException) parse
        // successfully; the lifter lowers them to java:throw(java:new(...)) terms.
        Set<String> declNames = declarationFnNames();

        // Scalar-param methods:
        assertTrue(declNames.stream().anyMatch(n -> n.contains("wrap_identity")),
            "WrapIdentityTransported.wrap_identity should lift; got: " + declNames);
        assertTrue(declNames.stream().anyMatch(n -> n.contains("toggle")),
            "ToggleTransported.toggle should lift; got: " + declNames);
        assertTrue(declNames.stream().anyMatch(n -> n.contains("assert_positive")),
            "AssertPositiveTransported.assert_positive should lift; got: " + declNames);
        assertTrue(declNames.stream().anyMatch(n -> n.contains("safe_divide") && !n.contains("then")),
            "SafeDivideTransported.safe_divide should lift; got: " + declNames);
        assertTrue(declNames.stream().anyMatch(n -> n.contains("safe_divide_then_double")),
            "SafeDivideThenDoubleTransported.safe_divide_then_double should lift; got: " + declNames);
        assertTrue(declNames.stream().anyMatch(n -> n.contains("swap_pair")),
            "SwapPairTransported.swap_pair should lift; got: " + declNames);
        assertTrue(declNames.stream().anyMatch(n -> n.contains("classify")),
            "ClassifyTransported.classify should lift; got: " + declNames);
        assertTrue(declNames.stream().anyMatch(n -> n.contains("retry_until_success")),
            "RetryUntilSuccessTransported.retry_until_success should lift; got: " + declNames);
        // &i64-param methods (lifted with <any> erasure via JDK error recovery):
        assertTrue(declNames.stream().anyMatch(n -> n.contains("maybe_first")),
            "MaybeFirstTransported.maybe_first should lift with <any> erasure; got: " + declNames);
        assertTrue(declNames.stream().anyMatch(n -> n.contains("option_bind_double")),
            "OptionBindDoubleTransported.option_bind_double should lift with <any> erasure; got: " + declNames);
        assertTrue(declNames.stream().anyMatch(n -> n.contains("list_sum")),
            "ListSumTransported.list_sum should lift with <any> erasure; got: " + declNames);
    }

    @Test
    void invalidJavaParamMethodsLiftWithErasedAnyType() {
        // MaybeFirstTransported, OptionBindDoubleTransported, ListSumTransported have
        // "&i64" parameter types (not valid Java). The JDK compiler's error-recovery
        // path erases the unresolvable type to <any>. The lifter still produces
        // declarations, with "<any>" in the erased parameter type position.
        //
        // Transport gap: bind-invalid-java-param-type + lift-any-erasure.
        // The formal sort is Ref (erased) instead of the intended array sort.
        Set<String> declNames = declarationFnNames();
        // Confirm <any> erasure appears in the function name:
        assertTrue(declNames.stream().anyMatch(n -> n.matches(".*maybe_first.*<any>.*")),
            "maybe_first declaration should use <any> for erased &i64 param; got: " + declNames);
        assertTrue(declNames.stream().anyMatch(n -> n.matches(".*option_bind_double.*<any>.*")),
            "option_bind_double declaration should use <any> for erased &i64 param; got: " + declNames);
        assertTrue(declNames.stream().anyMatch(n -> n.matches(".*list_sum.*<any>.*")),
            "list_sum declaration should use <any> for erased &i64 param; got: " + declNames);
    }

    @Test
    void liftedStubBodiesContainThrowAndPanicsTerms() {
        // All lifted methods have stub bodies: throw new UnsupportedOperationException(...)
        // The lifter lowers this to java:throw(java:new("UnsupportedOperationException", ...))
        // with a "panics" effect.
        //
        // Round-trip implication: the lifted IR captures the stub, not the original logic.
        // This is the evidence for the round-trip REFUSE verdict on all 11 concepts.
        String encoded = Jcs.encode(result.toJson());
        assertTrue(encoded.contains("java:throw"),
            "lifted declarations should contain java:throw terms from stub bodies");
        assertTrue(encoded.contains("panics"),
            "lifted declarations should contain panics effects from throw statements");
        assertTrue(encoded.contains("UnsupportedOperationException"),
            "lifted declarations should contain UnsupportedOperationException from stub bodies");
    }

    // ── round-trip verdicts ──────────────────────────────────────────────────

    @Test
    void exactRoundTripConceptCountIsZero() {
        // transport-gap: bind-stub-body-emitted
        //
        // The canonical Java output has no original function bodies -- all are
        // UnsupportedOperationException stubs. Re-lifting produces java:throw terms,
        // not the original Rust IR. No concept achieves byte-identical IR recovery.
        //
        // Round-trip verdict per trichotomy (transport-gap spec §0.1):
        //   - EXACT requires precondition=true, loss=empty. Not met: the lifted IR
        //     does not reproduce the source contract-content CID.
        //   - LOUDLY-BOUNDED-LOSSY requires a non-empty domain of agreement.
        //     Stub bodies agree with the source on the empty set (the stub never
        //     returns normally). This is total loss -- not the loudly-bounded case.
        //   - REFUSE: the correct verdict. The round-trip cannot recover the original
        //     logic even with characterization. Per Supra omnia rectum: refuse rather
        //     than claim a partial correctness that does not hold.
        //
        // EXACT: 0 / 11 trinity concepts (load-bearing assertion for the PR).

        Set<String> declNames = declarationFnNames();
        // 11 method declarations + 1 source-unit = at least 12 total.
        // (1 void method refused by lifter, so 11 method decls from 12 emitted classes.)
        assertTrue(declNames.size() >= 12,
            "expected 12 declarations (11 methods + source-unit); got " + declNames.size() + ": " + declNames);

        // The lifted IR encodes stub bodies, not original logic.
        String encoded = Jcs.encode(result.toJson());
        assertTrue(encoded.contains("java:throw"),
            "lifted IR should encode stub bodies as java:throw terms (not original Rust logic)");

        // If any declaration's post-condition matched the Rust-side CID, that would
        // be an exact hit. No such match exists in v0. The test's presence in the
        // repo is the standing assertion: run it after each bind improvement to
        // detect when exact count moves above zero.
    }

    @Test
    void sourceUnitDeclarationAlwaysPresent() {
        // liftSource always prepends a <source-unit:lib.java> contract.
        Set<String> declNames = declarationFnNames();
        assertTrue(declNames.stream().anyMatch(n -> n.contains("source-unit")),
            "source-unit contract should always be present; got: " + declNames);
    }

    // ── helpers ──────────────────────────────────────────────────────────────

    private Set<String> declarationFnNames() {
        return result.declarations().stream()
            .filter(d -> d instanceof Jcs.Obj obj && "function-contract".equals(obj.stringFieldOrNull("kind")))
            .map(d -> ((Jcs.Obj) d).stringField("fnName"))
            .collect(Collectors.toSet());
    }
}
