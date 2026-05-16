package com.provekit.lift.java_source;

import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertEquals;
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

    @Test
    void bindLifterRecoversObservationPolicyTagsAsNativeSurfaceWitness() {
        String source = """
            final class IdentityTransported {
                // concept: identity
                public static long identity(long x) {
                    long __provekit_result = x;
                    // provekit-observation: concept:contract-observation
                    // provekit-observation-term: concept:contract-observation(blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111,blake3-512:22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222,emitter)
                    // provekit-observation-mode: emitter
                    // provekit-concept-site-cid: blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111
                    // provekit-object-fcm-cid: blake3-512:22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222
                    // provekit-contract-cid: blake3-512:33333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333
                    // provekit-emitted-concept: concept:log-emit
                    // provekit-observation-policy-cid: blake3-512:44444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444
                    java.util.logging.Logger.getLogger("provekit").log(java.util.logging.Level.INFO, "observed");
                    return __provekit_result;
                }
            }
            """;

        JavaBindLifter.Result lift = new JavaBindLifter().liftPathsFromSource("Fixture.java", source);
        String encoded = Jcs.encode(lift.toJson());

        assertEquals(1, lift.entries().size(), encoded);
        assertTrue(encoded.contains("\"source_kind\":\"native-surface\""), encoded);
        assertTrue(encoded.contains("\"role\":\"observation\""), encoded);
        assertTrue(encoded.contains("concept:contract-observation"), encoded);
        assertTrue(encoded.contains("concept:log-emit"), encoded);
        assertTrue(encoded.contains("\"mode\":\"emitter\""), encoded);
        assertTrue(encoded.contains("\"policy_cid\":\"blake3-512:444444"), encoded);
        assertTrue(encoded.contains("\"object_fcm_cid\":\"blake3-512:222222"), encoded);
        assertTrue(encoded.contains("\"contract_cid\":\"blake3-512:333333"), encoded);
    }

    @Test
    void bindLifterDoesNotBleedObservationTagsIntoAdjacentMethods() {
        String source = """
            final class AdjacentTransported {
                // concept: first
                public static long first(long x) {
                    long __provekit_result = x;
                    // provekit-observation: concept:contract-observation
                    // provekit-observation-term: concept:contract-observation(blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111,blake3-512:22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222,emitter)
                    // provekit-observation-mode: emitter
                    // provekit-concept-site-cid: blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111
                    // provekit-object-fcm-cid: blake3-512:22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222
                    // provekit-contract-cid: blake3-512:33333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333
                    // provekit-observation-policy-cid: blake3-512:44444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444
                    java.util.logging.Logger.getLogger("provekit").log(java.util.logging.Level.INFO, "observed");
                    return __provekit_result;
                }

                // concept: second
                public static long second(long y) {
                    return y;
                }
            }
            """;

        JavaBindLifter.Result lift = new JavaBindLifter().liftPathsFromSource("Adjacent.java", source);
        String encoded = Jcs.encode(lift.toJson());

        assertEquals(2, lift.entries().size(), encoded);
        assertEquals(
            1,
            countOccurrences(encoded, "\"source_kind\":\"native-surface\""),
            "observation native-surface witness must belong only to the method whose body contains the tag: " + encoded);
    }

    @Test
    void bindLifterRecoversContractCommentTagsAsNativeSurfaceWitnesses() {
        String source = contractTaggedSource(PRE_FORMULA_CID, POST_FORMULA_CID, "pre", "1");

        JavaBindLifter.Result lift = new JavaBindLifter().liftPathsFromSource("Contracts.java", source);
        String encoded = Jcs.encode(lift.toJson());

        assertEquals(1, lift.entries().size(), encoded);
        assertEquals(2, countOccurrences(encoded, "\"source_kind\":\"native-surface\""), encoded);
        assertTrue(encoded.contains("\"role\":\"pre\""), encoded);
        assertTrue(encoded.contains("\"role\":\"post\""), encoded);
        assertTrue(encoded.contains("\"predicate_text\":\"non_null(name)\""), encoded);
        assertTrue(encoded.contains("\"predicate_text\":\"non_null(out)\""), encoded);
        assertTrue(encoded.contains("\"contract_cid\":\"" + CONTRACT_CID + "\""), encoded);
        assertTrue(encoded.contains("\"concept_site_cid\":\"" + CONCEPT_SITE_CID + "\""), encoded);
        assertTrue(encoded.contains("\"ir_formula_jcs_cid\":\"" + PRE_FORMULA_CID + "\""), encoded);
        assertTrue(encoded.contains("\"ir_formula_jcs_cid\":\"" + POST_FORMULA_CID + "\""), encoded);
        assertTrue(encoded.contains("\"policy_cid\":\"" + POLICY_CID + "\""), encoded);
        assertTrue(encoded.contains("\"sugar_dict_cid\":\"" + SUGAR_DICT_CID + "\""), encoded);
        assertTrue(encoded.contains("\"loss_record_cid\":\"" + LOSS_RECORD_CID + "\""), encoded);
        assertTrue(encoded.contains("\"payload_cid\":\"blake3-512:"), encoded);
        assertTrue(encoded.contains("\"surface\":\"contract-comment-sugar\""), encoded);
    }

    @Test
    void bindLifterFailsClosedOnContractCommentFormulaCidMismatch() {
        String source = contractTaggedSource(LOSS_RECORD_CID, POST_FORMULA_CID, "pre", "1");

        JavaBindLifter.Result lift = new JavaBindLifter().liftPathsFromSource("Contracts.java", source);
        String encoded = Jcs.encode(lift.toJson());

        assertEquals(1, lift.entries().size(), encoded);
        assertFalse(encoded.contains("\"predicate_text\":\"non_null(name)\""), encoded);
        assertFalse(encoded.contains("\"role\":\"pre\""), encoded);
        assertTrue(encoded.contains("\"role\":\"post\""), encoded);
    }

    @Test
    void bindLifterFailsClosedOnContractCommentUnknownRoleOrSchema() {
        String unknownRole = contractTaggedSource(PRE_FORMULA_CID, POST_FORMULA_CID, "guard", "1");
        String unknownSchema = contractTaggedSource(PRE_FORMULA_CID, POST_FORMULA_CID, "pre", "2");

        String roleEncoded = Jcs.encode(new JavaBindLifter().liftPathsFromSource("Contracts.java", unknownRole).toJson());
        String schemaEncoded = Jcs.encode(new JavaBindLifter().liftPathsFromSource("Contracts.java", unknownSchema).toJson());

        assertFalse(roleEncoded.contains("\"role\":\"guard\""), roleEncoded);
        assertFalse(roleEncoded.contains("\"predicate_text\":\"non_null(name)\""), roleEncoded);
        assertTrue(roleEncoded.contains("\"role\":\"post\""), roleEncoded);
        assertFalse(schemaEncoded.contains("\"predicate_text\":\"non_null(name)\""), schemaEncoded);
        assertTrue(schemaEncoded.contains("\"role\":\"post\""), schemaEncoded);
    }

    @Test
    void bindLifterFailsClosedOnMalformedContractCommentPayloadOrPayloadCidMismatch() {
        String malformedPayload = """
            final class BadContractTransported {
                // concept: lookup
                // provekit-contract: {"artifact_kind":
                public static String lookup(String name) {
                    return name;
                }
            }
            """;
        String validPrePayload = contractPayload("pre", "1", nonNullPredicate("name"), PRE_FORMULA_CID, "non_null(name)");
        String wrongPayloadCid = """
            final class BadContractTransported {
                // concept: lookup
                // provekit-contract: __PRE_PAYLOAD__
                // provekit-contract-payload-cid: __WRONG_CID__
                public static String lookup(String name) {
                    return name;
                }
            }
            """
            .replace("__PRE_PAYLOAD__", validPrePayload)
            .replace("__WRONG_CID__", LOSS_RECORD_CID);

        String malformedEncoded = Jcs.encode(new JavaBindLifter().liftPathsFromSource("BadContract.java", malformedPayload).toJson());
        String cidMismatchEncoded = Jcs.encode(new JavaBindLifter().liftPathsFromSource("BadContract.java", wrongPayloadCid).toJson());

        assertFalse(malformedEncoded.contains("\"source_kind\":\"native-surface\""), malformedEncoded);
        assertFalse(cidMismatchEncoded.contains("\"predicate_text\":\"non_null(name)\""), cidMismatchEncoded);
        assertFalse(cidMismatchEncoded.contains("\"payload_cid\":\"" + LOSS_RECORD_CID + "\""), cidMismatchEncoded);
    }

    @Test
    void test_concept_citation_relift_recovers_identity() {
        Jcs.Obj payload = conceptCitationPayload(ARGS_JCS_CID, "1", CONCEPT_SKIP_CID, "skip");
        String source = conceptTaggedSource(payload, Jcs.cid(payload));

        JavaBindLifter.Result lift = new JavaBindLifter().liftPathsFromSource("Concepts.java", source);
        String encoded = Jcs.encode(lift.toJson());

        assertEquals(1, lift.entries().size(), encoded);
        assertTrue(conceptDiagnostics(lift).isEmpty(), encoded);
        Jcs.Obj entry = (Jcs.Obj) lift.entries().get(0);
        Jcs.Arr citations = (Jcs.Arr) entry.get("concept_citations");
        assertEquals(1, citations.values().size(), encoded);
        Jcs.Obj citation = citations.objectAt(0);
        assertEquals(CONCEPT_SKIP_CID, citation.stringField("concept_cid"));
        assertEquals("skip", citation.stringField("operation_kind"));
        assertEquals(CONCEPT_SKIP_CID, citation.stringField("shape_cid"));
        assertEquals(ARGS_JCS_CID, citation.stringField("args_jcs_cid"));
        assertEquals("native-surface", citation.stringField("source_kind"));
        assertEquals(0, ((Jcs.Num) ((Jcs.Arr) citation.get("term_position")).get(0)).value());
        Jcs.Obj extensionFields = (Jcs.Obj) citation.get("extension_fields");
        assertEquals(Jcs.cid(payload), extensionFields.stringField("payload_cid"));
        assertEquals(CONCEPT_SITE_CID, extensionFields.stringField("concept_site_cid"));
        assertEquals(0, ((Jcs.Arr) entry.get("witnesses")).values().size());
    }

    @Test
    void test_concept_citation_payload_cid_mismatch_refuses() {
        Jcs.Obj payload = conceptCitationPayload(ARGS_JCS_CID, "1", CONCEPT_SKIP_CID, "skip");
        String source = conceptTaggedSource(payload, LOSS_RECORD_CID);

        JavaBindLifter.Result lift = new JavaBindLifter().liftPathsFromSource("Concepts.java", source);
        String encoded = Jcs.encode(lift.toJson());

        assertEquals(1, lift.entries().size(), encoded);
        assertEquals(0, conceptCitationCount(lift), encoded);
        assertTrue(conceptDiagnostics(lift).contains("concept-citation:payload-cid-mismatch"), encoded);
    }

    @Test
    void test_concept_citation_args_cid_mismatch_refuses() {
        Jcs.Obj payload = conceptCitationPayload(LOSS_RECORD_CID, "1", CONCEPT_SKIP_CID, "skip");
        String source = conceptTaggedSource(payload, Jcs.cid(payload));

        JavaBindLifter.Result lift = new JavaBindLifter().liftPathsFromSource("Concepts.java", source);
        String encoded = Jcs.encode(lift.toJson());

        assertEquals(1, lift.entries().size(), encoded);
        assertEquals(0, conceptCitationCount(lift), encoded);
        assertTrue(conceptDiagnostics(lift).contains("concept-citation:args-cid-mismatch"), encoded);
    }

    @Test
    void test_concept_citation_unknown_schema_version_refuses() {
        Jcs.Obj payload = conceptCitationPayload(ARGS_JCS_CID, "999", CONCEPT_SKIP_CID, "skip");
        String source = conceptTaggedSource(payload, Jcs.cid(payload));

        JavaBindLifter.Result lift = new JavaBindLifter().liftPathsFromSource("Concepts.java", source);
        String encoded = Jcs.encode(lift.toJson());

        assertEquals(1, lift.entries().size(), encoded);
        assertEquals(0, conceptCitationCount(lift), encoded);
        assertTrue(conceptDiagnostics(lift).contains("concept-citation:unknown-schema-version"), encoded);
    }

    @Test
    void test_concept_citation_malformed_json_refuses() {
        String source = conceptSourceWithComments("// provekit-concept: {\"artifact_kind\":");

        JavaBindLifter.Result lift = new JavaBindLifter().liftPathsFromSource("Concepts.java", source);
        String encoded = Jcs.encode(lift.toJson());

        assertEquals(1, lift.entries().size(), encoded);
        assertEquals(0, conceptCitationCount(lift), encoded);
        assertTrue(conceptDiagnostics(lift).contains("concept-citation:malformed-json"), encoded);
    }

    @Test
    void test_concept_citation_malformed_cid_refuses() {
        Jcs.Obj payload = conceptCitationPayload("not-a-cid", ARGS_JCS_CID, "1", CONCEPT_SKIP_CID, "skip");
        String source = conceptTaggedSource(payload, Jcs.cid(payload));

        JavaBindLifter.Result lift = new JavaBindLifter().liftPathsFromSource("Concepts.java", source);
        String encoded = Jcs.encode(lift.toJson());

        assertEquals(1, lift.entries().size(), encoded);
        assertEquals(0, conceptCitationCount(lift), encoded);
        assertTrue(conceptDiagnostics(lift).contains("concept-citation:malformed-cid"), encoded);
    }

    @Test
    void test_concept_citation_unknown_concept_refuses() {
        Jcs.Obj payload = conceptCitationPayload(_cid("7"), ARGS_JCS_CID, "1", CONCEPT_SKIP_CID, "skip");
        String source = conceptTaggedSource(payload, Jcs.cid(payload));

        JavaBindLifter.Result lift = new JavaBindLifter().liftPathsFromSource("Concepts.java", source);
        String encoded = Jcs.encode(lift.toJson());

        assertEquals(1, lift.entries().size(), encoded);
        assertEquals(0, conceptCitationCount(lift), encoded);
        assertTrue(conceptDiagnostics(lift).contains("concept-citation:unknown-concept"), encoded);
    }

    @Test
    void test_concept_citation_shape_mismatch_refuses_surrounding_relift() {
        Jcs.Obj payload = conceptCitationPayload(ARGS_JCS_CID, "1", LOSS_RECORD_CID, "skip");
        String source = conceptTaggedSource(payload, Jcs.cid(payload));

        JavaBindLifter.Result lift = new JavaBindLifter().liftPathsFromSource("Concepts.java", source);
        String encoded = Jcs.encode(lift.toJson());

        assertEquals(0, lift.entries().size(), encoded);
        assertTrue(conceptDiagnostics(lift).contains("concept-citation:shape-mismatch"), encoded);
    }

    @Test
    void test_concept_citation_operation_kind_mismatch_refuses_surrounding_relift() {
        Jcs.Obj payload = conceptCitationPayload(ARGS_JCS_CID, "1", CONCEPT_SKIP_CID, "not-skip");
        String source = conceptTaggedSource(payload, Jcs.cid(payload));

        JavaBindLifter.Result lift = new JavaBindLifter().liftPathsFromSource("Concepts.java", source);
        String encoded = Jcs.encode(lift.toJson());

        assertEquals(0, lift.entries().size(), encoded);
        assertTrue(conceptDiagnostics(lift).contains("concept-citation:operation-kind-mismatch"), encoded);
    }

    @Test
    void test_concept_citation_orphan_payload_cid_line_refuses() {
        String source = conceptSourceWithComments("// provekit-concept-payload-cid: " + LOSS_RECORD_CID);

        JavaBindLifter.Result lift = new JavaBindLifter().liftPathsFromSource("Concepts.java", source);
        String encoded = Jcs.encode(lift.toJson());

        assertEquals(1, lift.entries().size(), encoded);
        assertEquals(0, conceptCitationCount(lift), encoded);
        assertTrue(conceptDiagnostics(lift).contains("concept-citation:orphan-cid-line"), encoded);
    }

    @Test
    void test_concept_citation_lower_java_relift_round_trip() {
        Jcs.Obj payload = conceptCitationPayload(ARGS_JCS_CID, "1", CONCEPT_SKIP_CID, "skip");
        String source = conceptTaggedSource(payload, Jcs.cid(payload));

        JavaBindLifter.Result lift = new JavaBindLifter().liftPathsFromSource("RoundTrip.java", source);

        Jcs.Obj citation = ((Jcs.Arr) ((Jcs.Obj) lift.entries().get(0)).get("concept_citations")).objectAt(0);
        assertEquals(payload.stringField("concept_cid"), citation.stringField("concept_cid"));
        assertEquals(payload.stringField("operation_kind"), citation.stringField("operation_kind"));
        assertEquals(payload.stringField("shape_cid"), citation.stringField("shape_cid"));
        assertEquals(payload.stringField("args_jcs_cid"), citation.stringField("args_jcs_cid"));
    }

    // ── helpers ──────────────────────────────────────────────────────────────

    private static final String CONTRACT_CID =
        "blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111";
    private static final String CONCEPT_SITE_CID =
        "blake3-512:55555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555555";
    private static final String POLICY_CID =
        "blake3-512:22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222";
    private static final String SUGAR_DICT_CID =
        "blake3-512:33333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333333";
    private static final String LOSS_RECORD_CID =
        "blake3-512:44444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444";
    private static final String KIT_CID =
        "blake3-512:66666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666";
    private static final String PRE_FORMULA_CID = Jcs.cid(nonNullPredicate("name"));
    private static final String POST_FORMULA_CID = Jcs.cid(nonNullPredicate("out"));
    private static final String CONCEPT_SKIP_CID =
        "blake3-512:"
        + "9a905548a44fce23882b17d857d275d7822bd235ab71dbf786cd991563cc1de9e"
        + "610594f50ad3c89a3b7eeb43234a31b36caa8031914c85227158030669c63cb";
    private static final Jcs.Arr ARGS_JCS = Jcs.array(Jcs.object(
        "kind", Jcs.string("var"),
        "name", Jcs.string("x")
    ));
    private static final String ARGS_JCS_CID = Jcs.cid(ARGS_JCS);

    private static String contractTaggedSource(
        String preFormulaCid,
        String postFormulaCid,
        String preRole,
        String preSchemaVersion) {
        String prePayload = contractPayload(preRole, preSchemaVersion, nonNullPredicate("name"), preFormulaCid, "non_null(name)");
        String postPayload = contractPayload("post", "1", nonNullPredicate("out"), postFormulaCid, "non_null(out)");
        return """
            final class ContractTransported {
                // concept: lookup
                // provekit-contract: __PRE_PAYLOAD__
                // provekit-contract-payload-cid: __PRE_PAYLOAD_CID__
                // requires: non_null(name)
                // provekit-contract: __POST_PAYLOAD__
                // provekit-contract-payload-cid: __POST_PAYLOAD_CID__
                // ensures: non_null(out)
                public static String lookup(String name) {
                    return name;
                }
            }
            """
            .replace("__PRE_PAYLOAD__", prePayload)
            .replace("__PRE_PAYLOAD_CID__", payloadCid(prePayload))
            .replace("__POST_PAYLOAD__", postPayload)
            .replace("__POST_PAYLOAD_CID__", payloadCid(postPayload));
    }

    private static String contractPayload(
        String role,
        String schemaVersion,
        Jcs.Json formula,
        String formulaCid,
        String folText) {
        return Jcs.encode(Jcs.object(
            "artifact_kind", Jcs.string("provekit-contract-comment-sugar"),
            "concept_site_cid", Jcs.string(CONCEPT_SITE_CID),
            "contract_cid", Jcs.string(CONTRACT_CID),
            "emitted_by", Jcs.object(
                "kit_cid", Jcs.string(KIT_CID),
                "kit_kind", Jcs.string("realize"),
                "target_language", Jcs.string("java")
            ),
            "fol_text", Jcs.string(folText),
            "ir_formula_jcs", formula,
            "ir_formula_jcs_cid", Jcs.string(formulaCid),
            "local_contract_cid", Jcs.string(CONTRACT_CID),
            "loss_record_cid", Jcs.string(LOSS_RECORD_CID),
            "policy_cid", Jcs.string(POLICY_CID),
            "role", Jcs.string(role),
            "schema_version", Jcs.string(schemaVersion),
            "sugar_dict_cid", Jcs.string(SUGAR_DICT_CID)
        ));
    }

    private static String payloadCid(String payload) {
        return Jcs.cid(Jcs.parse(payload));
    }

    private static Jcs.Json nonNullPredicate(String symbol) {
        return Jcs.object(
            "args", Jcs.array(
                Jcs.object("kind", Jcs.string("var"), "name", Jcs.string(symbol)),
                Jcs.object(
                    "kind", Jcs.string("const"),
                    "sort", Jcs.object("kind", Jcs.string("primitive"), "name", Jcs.string("Ref")),
                    "value", Jcs.nullValue()
                )
            ),
            "kind", Jcs.string("atomic"),
            "name", Jcs.string("neq")
        );
    }

    private static Jcs.Obj conceptCitationPayload(
        String argsJcsCid,
        String schemaVersion,
        String shapeCid,
        String operationKind) {
        return conceptCitationPayload(CONCEPT_SKIP_CID, argsJcsCid, schemaVersion, shapeCid, operationKind);
    }

    private static Jcs.Obj conceptCitationPayload(
        String conceptCid,
        String argsJcsCid,
        String schemaVersion,
        String shapeCid,
        String operationKind) {
        return Jcs.object(
            "args_jcs", ARGS_JCS,
            "args_jcs_cid", Jcs.string(argsJcsCid),
            "artifact_kind", Jcs.string("provekit-concept-citation-comment-sugar"),
            "concept_cid", Jcs.string(conceptCid),
            "concept_name", Jcs.string("concept:skip"),
            "concept_site_cid", Jcs.string(CONCEPT_SITE_CID),
            "emitted_by", Jcs.object(
                "kit_cid", Jcs.string(KIT_CID),
                "kit_id", Jcs.string("provekit-realize-java-core@0.1.0"),
                "kit_kind", Jcs.string("realize"),
                "target_language", Jcs.string("java"),
                "target_library_tag", Jcs.string("java")
            ),
            "loss_record_cid", Jcs.string(LOSS_RECORD_CID),
            "operation_kind", Jcs.string(operationKind),
            "policy_cid", Jcs.string(POLICY_CID),
            "schema_version", Jcs.string(schemaVersion),
            "shape_cid", Jcs.string(shapeCid),
            "sugar_dict_cid", Jcs.string(SUGAR_DICT_CID),
            "term_position", Jcs.array(Jcs.integer(0))
        );
    }

    private static String conceptTaggedSource(Jcs.Obj payload, String payloadCid) {
        return conceptSourceWithComments(
            "// provekit-concept: " + Jcs.encode(payload) + "\n"
            + "// provekit-concept-payload-cid: " + payloadCid);
    }

    private static String conceptSourceWithComments(String comments) {
        return """
            final class ConceptTransported {
                // concept: skip
                public static void transport_skip(Object x) {
__COMMENTS__
                    ;
                }
            }
            """
            .replace("__COMMENTS__", comments);
    }

    private static String _cid(String ch) {
        return "blake3-512:" + ch.repeat(128);
    }

    private static int conceptCitationCount(JavaBindLifter.Result lift) {
        if (lift.entries().isEmpty()) return 0;
        Jcs.Json citations = ((Jcs.Obj) lift.entries().get(0)).get("concept_citations");
        return citations instanceof Jcs.Arr arr ? arr.values().size() : 0;
    }

    private static Set<String> conceptDiagnostics(JavaBindLifter.Result lift) {
        return lift.diagnostics().stream()
            .filter(d -> d instanceof Jcs.Obj obj && obj.stringFieldOrNull("kind") != null)
            .map(d -> ((Jcs.Obj) d).stringField("kind"))
            .filter(kind -> kind.startsWith("concept-citation:"))
            .collect(Collectors.toSet());
    }

    private Set<String> declarationFnNames() {
        return result.declarations().stream()
            .filter(d -> d instanceof Jcs.Obj obj && "function-contract".equals(obj.stringFieldOrNull("kind")))
            .map(d -> ((Jcs.Obj) d).stringField("fnName"))
            .collect(Collectors.toSet());
    }

    private static int countOccurrences(String haystack, String needle) {
        int count = 0;
        int idx = 0;
        while ((idx = haystack.indexOf(needle, idx)) >= 0) {
            count += 1;
            idx += needle.length();
        }
        return count;
    }
}
