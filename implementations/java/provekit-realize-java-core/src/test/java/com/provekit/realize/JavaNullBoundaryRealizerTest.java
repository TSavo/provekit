package com.provekit.realize;

import java.nio.charset.StandardCharsets;

import com.provekit.ir.Blake3;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

public class JavaNullBoundaryRealizerTest {
    private static final String LAB_SOURCE = """
        package zoo;

        public final class UserDirectory {
            public String lookup(String name) {
                return "user:" + name.toUpperCase();
            }
        }
        """;

    @Test
    public void provekitNativeTransformIsClosedOnlyAfterReliftSeesRequires() {
        RealizerPlan plan = RealizerPlan.transform(
            "blake3-512:gap",
            "maybe_null(name)",
            "non_null(name)",
            "blake3-512:policy",
            "java-provekit-native",
            "lookup",
            "name",
            LAB_SOURCE
        );

        RealizerOutput output = new JavaNullBoundaryRealizer().realize(plan);

        assertEquals("closed", output.status());
        assertEquals("transform", output.mode());
        assertTrue(output.modifiedSource().contains("import com.provekit.contract.Requires;"));
        assertTrue(output.modifiedSource().contains("@Requires(\"name != null\")"));
        assertTrue(output.postLiftJson().contains("\"name\":\"neq\""));
        assertTrue(output.postLiftJson().contains("\"value\":null"));
        assertTrue(output.hasClosedInvariantEvidence(), "closed output must carry all ORP invariant evidence");
        assertTrue(output.transformedArtifactCid().startsWith("blake3-512:"));
        assertTrue(output.postLiftCid().startsWith("blake3-512:"));
        assertTrue(output.closureWitnessCid().startsWith("blake3-512:"));
        assertEquals(
            output.closureWitnessCid(),
            Blake3.blake3_512(output.closureWitnessJson().getBytes(StandardCharsets.UTF_8))
        );
        assertTrue(output.toJson().contains("\"closureWitness\":{"));
    }

    @Test
    public void springWebTransformClosesWithRequestParamSurface() {
        RealizerPlan plan = RealizerPlan.transform(
            "blake3-512:gap",
            "maybe_null(name)",
            "non_null(name)",
            "blake3-512:policy",
            "java-spring-web",
            "lookup",
            "name",
            LAB_SOURCE
        );

        RealizerOutput output = new JavaNullBoundaryRealizer().realize(plan);

        assertEquals("closed", output.status());
        assertTrue(output.modifiedSource().contains("import org.springframework.web.bind.annotation.RequestParam;"));
        assertTrue(output.modifiedSource().contains("lookup(@RequestParam String name)"));
        assertTrue(output.postLiftJson().contains("\"name\":\"neq\""));
        assertTrue(output.hasClosedInvariantEvidence(), "closed Spring output must carry closure evidence");
    }

    @Test
    public void unsupportedSurfaceRefusesInsteadOfPretendingToClose() {
        RealizerPlan plan = RealizerPlan.transform(
            "blake3-512:gap",
            "maybe_null(name)",
            "non_null(name)",
            "blake3-512:policy",
            "java-unknown",
            "lookup",
            "name",
            LAB_SOURCE
        );

        RealizerOutput output = new JavaNullBoundaryRealizer().realize(plan);

        assertEquals("rejected", output.status());
        assertFalse(output.hasClosedInvariantEvidence(), "refusal must not look like closure evidence");
        assertNull(output.closureWitnessCid());
    }

    @Test
    public void provekitNativeRefusesUnboundProofVariable() {
        RealizerPlan plan = RealizerPlan.transform(
            "blake3-512:gap",
            "maybe_null(email)",
            "non_null(email)",
            "blake3-512:policy",
            "java-provekit-native",
            "lookup",
            "email",
            LAB_SOURCE
        );

        RealizerOutput output = new JavaNullBoundaryRealizer().realize(plan);

        assertEquals("rejected", output.status());
        assertFalse(output.hasClosedInvariantEvidence(), "unbound proof variable must not close");
        assertNull(output.closureWitnessCid());
        assertFalse(output.modifiedSource().contains("@Requires(\"email != null\")"));
    }

    @Test
    public void bindContractWitnessesEmitNotNullSugar() {
        ContractPayload contract = new ContractPayload(
            "blake3-512:site",
            "blake3-512:compound",
            "evidence-lift[type-signature]",
            "exact",
            java.util.List.of(
                new ContractWitness("pre", "non_null(name)", "type-signature"),
                new ContractWitness("post", "non_null(out)", "type-signature")
            )
        );

        SugarRealizer.Realization output = SugarRealizer.emitStub(
            "lookup",
            java.util.List.of("name"),
            java.util.List.of("String"),
            "String",
            "concept:lookup",
            "witness",
            java.util.List.of("witness", "gate"),
            contract,
            java.util.List.of(modeScopedBeanValidationSugar("gate"), modeScopedJunitSugar("witness"), commentSugar())
        );

        assertTrue(output.source().contains("import jakarta.validation.constraints.NotNull;"));
        assertTrue(output.source().contains("import org.junit.jupiter.api.Disabled;"));
        assertTrue(output.source().contains("import static org.junit.jupiter.api.Assertions.assertNotNull;"));
        assertTrue(output.source().contains("@NotNull\n    public static String lookup(@NotNull String name)"));
        assertTrue(output.source().contains("@Disabled(\"provekit witness skeleton requires concrete values\")"));
        assertTrue(output.source().contains("Object name = null;"));
        assertTrue(output.source().contains("assertNotNull(name);"));
        assertTrue(output.source().contains("// requires: non_null(name)"));
        assertTrue(output.source().contains("// ensures: non_null(out)"));
        assertTrue(output.source().contains("// contract-cid: blake3-512:compound"));
        assertTrue(output.source().contains("// contract-source: type-signature"));
        assertTrue(output.observedLossRecord().contains("witness_requires_test_execution"));
        assertTrue(output.observedLossRecord().contains("witness_skeleton_requires_concrete_values"));
        assertTrue(output.observedLossRecord().contains("machine_uncheckable_prose"));
        assertTrue(output.usedSugarsJson().contains("java-bean-validation"));
        assertTrue(output.usedSugarsJson().contains("java-junit5"));
        assertTrue(output.usedSugarsJson().contains("java-function-comment"));
    }

    @Test
    public void contractCommentSugarEmitsReliftablePreAndPostPayloads() {
        String contractCid = "blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111";
        ContractPayload contract = new ContractPayload(
            "blake3-512:22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222",
            contractCid,
            "evidence-lift[native-surface]",
            "loudly-bounded-lossy",
            java.util.List.of(
                new ContractWitness("pre", "non_null(name)", "native-surface"),
                new ContractWitness("post", "non_null(out)", "native-surface")
            )
        );

        SugarRealizer.Realization output = SugarRealizer.emitStub(
            "lookup",
            java.util.List.of("name"),
            java.util.List.of("String"),
            "String",
            "concept:lookup",
            "monitor",
            java.util.List.of("monitor"),
            contract,
            java.util.List.of(validCommentSugar())
        );

        assertEquals(2, countOccurrences(output.source(), "// provekit-contract: {"), output.source());
        assertEquals(2, countOccurrences(output.source(), "// provekit-contract-payload-cid: blake3-512:"), output.source());
        assertTrue(output.source().contains("\"artifact_kind\":\"provekit-contract-comment-sugar\""), output.source());
        assertTrue(output.source().contains("\"schema_version\":\"1\""), output.source());
        assertTrue(output.source().contains("\"role\":\"pre\""), output.source());
        assertTrue(output.source().contains("\"role\":\"post\""), output.source());
        assertTrue(output.source().contains("\"contract_cid\":\"" + contractCid + "\""), output.source());
        assertEquals(2, countOccurrences(output.source(), "\"ir_formula_jcs_cid\":\"blake3-512:"), output.source());
        assertEquals(2, countOccurrences(output.source(), "\"policy_cid\":\"blake3-512:"), output.source());
        assertEquals(2, countOccurrences(output.source(), "\"sugar_dict_cid\":\"blake3-512:"), output.source());
        assertEquals(2, countOccurrences(output.source(), "\"loss_record_cid\":\"blake3-512:"), output.source());
        assertTrue(output.source().contains("\"fol_text\":\"non_null(name)\""), output.source());
        assertTrue(output.source().contains("\"fol_text\":\"non_null(out)\""), output.source());
    }

    @Test
    public void test_concept_citation_comment_emitted_for_transported_operation() {
        TransportedOperation transportedOp = transportedOperation();

        SugarRealizer.Realization output = SugarRealizer.emitStub(
            "transport_skip",
            java.util.List.of("x"),
            java.util.List.of("Object"),
            "()",
            "missing-java-skip-carrier",
            "monitor",
            java.util.List.of("monitor"),
            null,
            java.util.List.of(),
            transportedOp
        );

        assertTrue(output.source().contains("// provekit-concept: {"), output.source());
        assertTrue(output.source().contains("// provekit-concept-payload-cid: blake3-512:"), output.source());
        assertTrue(output.source().contains("        ;\n"), output.source());
        assertFalse(output.source().contains("UnsupportedOperationException"), output.source());
        assertTrue(
            output.source().indexOf("// provekit-concept:")
                < output.source().indexOf("        ;"),
            output.source()
        );

        String payloadText = conceptPayloadLine(output.source());
        com.provekit.ir.Jcs.Obj payload = (com.provekit.ir.Jcs.Obj) com.provekit.ir.Jcs.parse(payloadText);
        assertEquals("provekit-concept-citation-comment-sugar", payload.stringField("artifact_kind"));
        assertEquals("1", payload.stringField("schema_version"));
        assertEquals(transportedOp.conceptCid(), payload.stringField("concept_cid"));
        assertEquals(transportedOp.conceptName(), payload.stringField("concept_name"));
        assertEquals(transportedOp.conceptSiteCid(), payload.stringField("concept_site_cid"));
        assertEquals(transportedOp.lossRecordCid(), payload.stringField("loss_record_cid"));
        assertEquals(transportedOp.operationKind(), payload.stringField("operation_kind"));
        assertEquals(transportedOp.policyCid(), payload.stringField("policy_cid"));
        assertEquals(transportedOp.shapeCid(), payload.stringField("shape_cid"));
        assertEquals(transportedOp.sugarDictCid(), payload.stringField("sugar_dict_cid"));
        assertEquals(transportedOp.callsiteCid(), payload.stringField("callsite_cid"));
        assertEquals(com.provekit.ir.Jcs.cid(transportedOp.argsJcs()), payload.stringField("args_jcs_cid"));

        com.provekit.ir.Jcs.Arr termPosition = (com.provekit.ir.Jcs.Arr) payload.get("term_position");
        assertEquals(1, termPosition.values().size());
        assertEquals(0, ((com.provekit.ir.Jcs.Num) termPosition.get(0)).value());
        com.provekit.ir.Jcs.Obj emittedBy = (com.provekit.ir.Jcs.Obj) payload.get("emitted_by");
        assertTrue(emittedBy.stringField("kit_cid").startsWith("blake3-512:"));
        assertEquals("provekit-realize-java-core@0.1.0", emittedBy.stringField("kit_id"));
        assertEquals("realize", emittedBy.stringField("kit_kind"));
        assertEquals("java", emittedBy.stringField("target_language"));
        assertEquals("java", emittedBy.stringField("target_library_tag"));

        String payloadCid = com.provekit.ir.Jcs.cid(payload);
        assertTrue(output.source().contains("// provekit-concept-payload-cid: " + payloadCid), output.source());
    }

    @Test
    public void addr_named_term_tree_derives_concept_citation_carrier_loss() {
        TransportedOperation transportedOp = TransportedOperation.fromNamedTermTree(namedTermTree(
            "concept:addr",
            "addr",
            cid("1")
        ));

        assertNotNull(transportedOp);
        SugarRealizer.Realization output = SugarRealizer.emitStub(
            "addr_carrier",
            java.util.List.of("target"),
            java.util.List.of("Object"),
            "Object",
            "concept:addr",
            "monitor",
            java.util.List.of("monitor"),
            null,
            java.util.List.of(),
            transportedOp
        );

        assertTrue(output.source().contains("// provekit-concept: {"), output.source());
        assertEquals(carrierLossRecordJson("java-references-not-addresses"), output.observedLossRecord());

        String payloadText = conceptPayloadLine(output.source());
        com.provekit.ir.Jcs.Obj payload = (com.provekit.ir.Jcs.Obj) com.provekit.ir.Jcs.parse(payloadText);
        assertEquals("concept:addr", payload.stringField("concept_name"));
        assertEquals("addr", payload.stringField("operation_kind"));
        assertEquals(transportedOp.lossRecordCid(), payload.stringField("loss_record_cid"));
        assertEquals(
            com.provekit.ir.Jcs.cid((com.provekit.ir.Jcs.Json) com.provekit.ir.Jcs.parse(output.observedLossRecord())),
            payload.stringField("loss_record_cid")
        );
    }

    @Test
    public void deref_named_term_tree_derives_concept_citation_carrier_loss() {
        TransportedOperation transportedOp = TransportedOperation.fromNamedTermTree(namedTermTree(
            "concept:deref",
            "deref",
            cid("2")
        ));

        assertNotNull(transportedOp);
        SugarRealizer.Realization output = SugarRealizer.emitStub(
            "deref_carrier",
            java.util.List.of("ref"),
            java.util.List.of("Object"),
            "Object",
            "concept:deref",
            "monitor",
            java.util.List.of("monitor"),
            null,
            java.util.List.of(),
            transportedOp
        );

        assertTrue(output.source().contains("// provekit-concept: {"), output.source());
        assertEquals(carrierLossRecordJson("java-implicit-deref"), output.observedLossRecord());

        String payloadText = conceptPayloadLine(output.source());
        com.provekit.ir.Jcs.Obj payload = (com.provekit.ir.Jcs.Obj) com.provekit.ir.Jcs.parse(payloadText);
        assertEquals("concept:deref", payload.stringField("concept_name"));
        assertEquals("deref", payload.stringField("operation_kind"));
        assertEquals(transportedOp.lossRecordCid(), payload.stringField("loss_record_cid"));
        assertEquals(
            com.provekit.ir.Jcs.cid((com.provekit.ir.Jcs.Json) com.provekit.ir.Jcs.parse(output.observedLossRecord())),
            payload.stringField("loss_record_cid")
        );
    }

    @Test
    public void modeScopedJunitWitnessSugarDoesNotApplyToMonitor() {
        ContractPayload contract = new ContractPayload(
            "blake3-512:site",
            "blake3-512:compound",
            "evidence-lift[type-signature]",
            "exact",
            java.util.List.of(new ContractWitness("pre", "non_null(name)", "type-signature"))
        );

        SugarRealizer.Realization output = SugarRealizer.emitStub(
            "lookup",
            java.util.List.of("name"),
            java.util.List.of("String"),
            "String",
            "concept:lookup",
            "monitor",
            contract,
            java.util.List.of(modeScopedJunitSugar("witness"))
        );

        assertFalse(output.source().contains("org.junit.jupiter.api"));
        assertFalse(output.source().contains("WitnessTest"));
        assertFalse(output.source().contains("assertNotNull(name);"));
        assertFalse(output.usedSugarsJson().contains("java-junit5"));
    }

    @Test
    public void bindContractNumericWitnessesEmitMinMaxGateSugar() {
        ContractPayload contract = new ContractPayload(
            "blake3-512:site",
            "blake3-512:compound",
            "evidence-lift[test-assertion]",
            "exact",
            java.util.List.of(
                new ContractWitness("pre", numericPredicate("gt", "age", 0), "age > 0", "test-assertion"),
                new ContractWitness("pre", numericPredicate("le", "age", 130), "age <= 130", "test-assertion")
            )
        );

        SugarRealizer.Realization output = SugarRealizer.emitStub(
            "admit",
            java.util.List.of("age"),
            java.util.List.of("i32"),
            "()",
            "concept:admit",
            "gate",
            java.util.List.of("gate"),
            contract,
            java.util.List.of(modeScopedBeanValidationNumericSugar("gate"))
        );

        assertTrue(output.source().contains("import jakarta.validation.constraints.Min;"));
        assertTrue(output.source().contains("import jakarta.validation.constraints.Max;"));
        assertTrue(output.source().contains("public static void admit(@Min(1) @Max(130) int age)"));
        assertTrue(output.usedSugarsJson().contains("java-bean-validation"));
        assertEquals("{}", output.observedLossRecord());
    }

    @Test
    public void strictNumericBoundOverflowDoesNotEmitWrappedGateSugar() {
        ContractPayload contract = new ContractPayload(
            "blake3-512:site",
            "blake3-512:compound",
            "evidence-lift[test-assertion]",
            "exact",
            java.util.List.of(
                new ContractWitness("pre", numericPredicate("gt", "age", Long.MAX_VALUE), "age > Long.MAX_VALUE", "test-assertion"),
                new ContractWitness("pre", numericPredicate("lt", "score", Long.MIN_VALUE), "score < Long.MIN_VALUE", "test-assertion")
            )
        );

        SugarRealizer.Realization output = SugarRealizer.emitStub(
            "admit",
            java.util.List.of("age", "score"),
            java.util.List.of("i64", "i64"),
            "()",
            "concept:admit",
            "gate",
            java.util.List.of("gate"),
            contract,
            java.util.List.of(modeScopedBeanValidationNumericSugar("gate"))
        );

        assertFalse(output.source().contains("@Min("));
        assertFalse(output.source().contains("@Max("));
        assertFalse(output.source().contains("import jakarta.validation.constraints.Min;"));
        assertFalse(output.source().contains("import jakarta.validation.constraints.Max;"));
        assertFalse(output.usedSugarsJson().contains("java-bean-validation"));
        assertEquals("{}", output.observedLossRecord());
    }

    @Test
    public void inclusiveNumericBoundsAtLongExtremaStillEmitExactGateSugar() {
        ContractPayload contract = new ContractPayload(
            "blake3-512:site",
            "blake3-512:compound",
            "evidence-lift[test-assertion]",
            "exact",
            java.util.List.of(
                new ContractWitness("pre", numericPredicate("ge", "age", Long.MAX_VALUE), "age >= Long.MAX_VALUE", "test-assertion"),
                new ContractWitness("pre", numericPredicate("le", "score", Long.MIN_VALUE), "score <= Long.MIN_VALUE", "test-assertion")
            )
        );

        SugarRealizer.Realization output = SugarRealizer.emitStub(
            "admit",
            java.util.List.of("age", "score"),
            java.util.List.of("i64", "i64"),
            "()",
            "concept:admit",
            "gate",
            java.util.List.of("gate"),
            contract,
            java.util.List.of(modeScopedBeanValidationNumericSugar("gate"))
        );

        assertTrue(output.source().contains("import jakarta.validation.constraints.Min;"));
        assertTrue(output.source().contains("import jakarta.validation.constraints.Max;"));
        assertTrue(output.source().contains("@Min(9223372036854775807) long age"));
        assertTrue(output.source().contains("@Max(-9223372036854775808) long score"));
        assertTrue(output.usedSugarsJson().contains("java-bean-validation"));
        assertEquals("{}", output.observedLossRecord());
    }

    @Test
    public void contractObservationWitnessBodyTemplateEmitsWitnessCall() {
        java.util.Optional<String> body = SugarRealizer.bodyTemplateFor(
            "concept:contract-observation",
            java.util.List.of("callsiteCid", "contractCid", "mode"),
            "witness"
        );

        assertTrue(body.isPresent());
        assertTrue(body.get().contains("provekit_witness.observe"));
        assertTrue(body.get().contains("callsiteCid"));
        assertTrue(body.get().contains("contractCid"));
        assertTrue(body.get().contains("mode"));
    }

    @Test
    void contractObservationGateModeDoesNotRenderWitnessBodyTemplate() {
        java.util.Optional<String> body = SugarRealizer.bodyTemplateFor(
            "concept:contract-observation",
            java.util.List.of("callsiteCid", "contractCid", "mode"),
            "gate"
        );

        assertTrue(body.isEmpty());
    }

    @Test
    public void logEmitBodyTemplateRendersJavaUtilLoggingCall() {
        java.util.Optional<String> body = SugarRealizer.bodyTemplateFor(
            "concept:log-emit",
            java.util.List.of("info", "\"observed \" + callsiteCid", "contractCid"),
            "emitter"
        );

        assertTrue(body.isPresent());
        assertTrue(body.get().contains("java.util.logging.Logger.getLogger(\"provekit\")"));
        assertTrue(body.get().contains("java.util.logging.Level.INFO"));
        assertTrue(body.get().contains("\"observed \" + callsiteCid"));
        assertFalse(body.get().contains("${"));
    }

    @Test
    public void logEmitBodyTemplateMapsCanonicalLevelsToJavaUtilLoggingLevels() {
        java.util.Optional<String> warnBody = SugarRealizer.bodyTemplateFor(
            "concept:log-emit",
            java.util.List.of("warn", "\"warned\"", "contractCid"),
            "emitter"
        );
        java.util.Optional<String> fatalBody = SugarRealizer.bodyTemplateFor(
            "concept:log-emit",
            java.util.List.of("fatal", "\"failed\"", "contractCid"),
            "emitter"
        );

        assertTrue(warnBody.isPresent());
        assertTrue(warnBody.get().contains("java.util.logging.Level.WARNING"));
        assertFalse(warnBody.get().contains("Level.warn"));
        assertTrue(fatalBody.isPresent());
        assertTrue(fatalBody.get().contains("java.util.logging.Level.SEVERE"));
        assertFalse(fatalBody.get().contains("Level.fatal"));
    }

    @Test
    public void contractObservationMonitorRecursivelyRendersLogEmitCitation() {
        java.util.Optional<String> body = SugarRealizer.bodyTemplateFor(
            "concept:contract-observation",
            java.util.List.of("callsiteCid", "contractCid", "mode"),
            "monitor"
        );

        assertTrue(body.isPresent());
        assertTrue(body.get().contains("java.util.logging.Logger.getLogger(\"provekit\")"));
        assertTrue(body.get().contains("java.util.logging.Level.INFO"));
        assertTrue(body.get().contains("\"observed \" + callsiteCid + \" contract \" + contractCid"));
        assertTrue(body.get().contains("return null;"));
        assertFalse(body.get().contains("${log_emit}"));
        assertFalse(body.get().contains("concept:log-emit"));
    }

    @Test
    public void emitStubForContractObservationMonitorUsesRecursiveLogEmitBody() {
        SugarRealizer.Realization output = SugarRealizer.emitStub(
            "observe_contract",
            java.util.List.of("callsiteCid", "contractCid", "mode"),
            java.util.List.of("String", "String", "String"),
            "ContractObservationResult",
            "concept:contract-observation",
            "monitor",
            null
        );

        assertFalse(output.isStub());
        assertTrue(output.source().contains("java.util.logging.Logger.getLogger(\"provekit\")"));
        assertTrue(output.source().contains("java.util.logging.Level.INFO"));
        assertTrue(output.source().contains("return null;"));
        assertFalse(output.source().contains("${log_emit}"));
        assertTrue(output.observedLossRecord().contains("requires-java-util-logging-runtime"));
        assertTrue(output.observedLossRecord().contains("java-util-logging-formats-structured-fields"));
        assertTrue(output.observedLossRecord().contains("java-util-logging-level-taxonomy"));
    }

    @Test
    public void ordinaryIdentityEmitterComposesRecursiveObservationAfterReturnWithoutChangingResult() {
        ContractPayload contract = new ContractPayload(
            "blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111",
            "blake3-512:22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222",
            "evidence-lift[type-signature]",
            "exact",
            java.util.List.of(new ContractWitness("pre", "x >= 0", "type-signature"))
        );

        SugarRealizer.Realization output = SugarRealizer.emitStub(
            "identity",
            java.util.List.of("x"),
            java.util.List.of("i64"),
            "i64",
            "identity",
            "emitter",
            java.util.List.of("emitter"),
            contract,
            java.util.List.of()
        );

        assertFalse(output.isStub());
        assertTrue(output.source().contains("long __provekit_result = x;"), output.source());
        assertTrue(output.source().contains("// provekit-observation: concept:contract-observation"), output.source());
        assertTrue(output.source().contains("// provekit-observation-mode: emitter"), output.source());
        assertTrue(output.source().contains("// provekit-object-fcm-cid: blake3-512:222222"), output.source());
        assertTrue(output.source().contains("// provekit-observation-policy-cid: blake3-512:"), output.source());
        assertTrue(output.source().contains("java.util.logging.Logger.getLogger(\"provekit\")"), output.source());
        assertTrue(output.source().contains("java.util.logging.Level.INFO"), output.source());
        assertTrue(output.source().contains("return __provekit_result;"), output.source());
        assertFalse(output.source().contains("return null;"), output.source());
        assertFalse(output.source().contains("// provekit-wrapper-fcm-cid:"), output.source());
        assertTrue(
            output.source().indexOf("long __provekit_result = x;")
                < output.source().indexOf("// provekit-observation: concept:contract-observation"),
            output.source()
        );
        assertTrue(
            output.source().indexOf("// provekit-observation: concept:contract-observation")
                < output.source().indexOf("java.util.logging.Logger.getLogger(\"provekit\")"),
            output.source()
        );
        assertTrue(
            output.source().indexOf("java.util.logging.Logger.getLogger(\"provekit\")")
                < output.source().indexOf("return __provekit_result;"),
            output.source()
        );
        assertTrue(output.observedLossRecord().contains("requires-java-util-logging-runtime"));
        assertTrue(output.observedLossRecord().contains("java-util-logging-formats-structured-fields"));
        assertTrue(output.observedLossRecord().contains("java-util-logging-level-taxonomy"));
        assertNotNull(output.observationWrapperEmissionRecord());
        assertTrue(output.observationWrapperEmissionRecord().contains("\"occurrence_kind\":\"Io\""));
        assertTrue(output.observationWrapperEmissionRecord().contains("\"wrapper_fcm_cid\":\"blake3-512:"));
        assertTrue(output.observationWrapperEmissionRecord().contains("\"preservation_claim_cid\":\"blake3-512:"));
        assertFalse(output.observationWrapperEmissionRecord().contains("placeholder"));
    }

    @Test
    public void ordinaryIdentityMonitorDoesNotComposeEmitterObservationWrapper() {
        ContractPayload contract = new ContractPayload(
            "blake3-512:11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111",
            "blake3-512:22222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222222",
            "evidence-lift[type-signature]",
            "exact",
            java.util.List.of(new ContractWitness("pre", "x >= 0", "type-signature"))
        );

        SugarRealizer.Realization output = SugarRealizer.emitStub(
            "identity",
            java.util.List.of("x"),
            java.util.List.of("i64"),
            "i64",
            "identity",
            "monitor",
            java.util.List.of("monitor"),
            contract,
            java.util.List.of()
        );

        assertFalse(output.isStub());
        assertTrue(output.source().contains("return x;"), output.source());
        assertFalse(output.source().contains("__provekit_result"), output.source());
        assertFalse(output.source().contains("java.util.logging.Logger.getLogger(\"provekit\")"), output.source());
        assertNull(output.observationWrapperEmissionRecord());
    }

    private static String modeScopedBeanValidationSugar(String mode) {
        return "{\"header\":{\"cid\":\"java-bean-validation\",\"content\":{\"entries\":[{\"emission_template\":{\"kind\":\"verbatim\",\"surface_locator\":\"annotation:before-parameter\",\"template\":\"@NotNull\"},\"loss_record_contribution\":{\"form\":\"literal\",\"value\":{}},\"mode\":\"" + mode + "\",\"predicate_pattern\":{\"args\":[{\"kind\":\"var\",\"name\":\"${symbol}\"},{\"kind\":\"const\",\"sort\":{\"kind\":\"primitive\",\"name\":\"Ref\"},\"value\":null}],\"kind\":\"atomic\",\"name\":\"neq\"}}],\"sugar_name\":\"bean-validation\",\"target_language\":\"java\"},\"critical\":false,\"kind\":\"sugar\",\"protocol_versions\":[\"pep/1.7.0\"],\"provenance_cid\":\"blake3-512:0\",\"schemaVersion\":\"1\",\"version\":\"1.0.0\"}}";
    }

    private static String modeScopedBeanValidationNumericSugar(String mode) {
        return """
            {"header":{"cid":"java-bean-validation","content":{"entries":[
              {"emission_template":{"kind":"verbatim","surface_locator":"annotation:before-parameter","template":"@Min(${k})"},"loss_record_contribution":{"form":"literal","value":{}},"mode":"__MODE__","predicate_pattern":{"args":[{"kind":"var","name":"${symbol}"},{"kind":"const","sort":{"kind":"primitive","name":"Int"},"value":"${k}"}],"kind":"atomic","name":"ge"}},
              {"emission_template":{"kind":"verbatim","surface_locator":"annotation:before-parameter","template":"@Min(${k_plus_one})"},"loss_record_contribution":{"form":"literal","value":{}},"mode":"__MODE__","predicate_pattern":{"args":[{"kind":"var","name":"${symbol}"},{"kind":"const","sort":{"kind":"primitive","name":"Int"},"value":"${k}"}],"kind":"atomic","name":"gt"}},
              {"emission_template":{"kind":"verbatim","surface_locator":"annotation:before-parameter","template":"@Max(${k})"},"loss_record_contribution":{"form":"literal","value":{}},"mode":"__MODE__","predicate_pattern":{"args":[{"kind":"var","name":"${symbol}"},{"kind":"const","sort":{"kind":"primitive","name":"Int"},"value":"${k}"}],"kind":"atomic","name":"le"}},
              {"emission_template":{"kind":"verbatim","surface_locator":"annotation:before-parameter","template":"@Max(${k_minus_one})"},"loss_record_contribution":{"form":"literal","value":{}},"mode":"__MODE__","predicate_pattern":{"args":[{"kind":"var","name":"${symbol}"},{"kind":"const","sort":{"kind":"primitive","name":"Int"},"value":"${k}"}],"kind":"atomic","name":"lt"}}
            ],"sugar_name":"bean-validation","target_language":"java"},"critical":false,"kind":"sugar","protocol_versions":["pep/1.7.0"],"provenance_cid":"blake3-512:0","schemaVersion":"1","version":"1.0.0"}}
            """.replace("__MODE__", mode);
    }

    private static String modeScopedJunitSugar(String mode) {
        return "{\"header\":{\"cid\":\"java-junit5\",\"content\":{\"entries\":[{\"emission_template\":{\"kind\":\"verbatim\",\"surface_locator\":\"witness:junit5-test\",\"template\":\"assertNotNull(${symbol});\"},\"loss_record_contribution\":{\"form\":\"literal\",\"value\":{\"domain_narrowing\":{\"args\":[],\"kind\":\"atomic\",\"name\":\"witness_requires_test_execution\"},\"structural_divergence\":{\"args\":[],\"kind\":\"atomic\",\"name\":\"witness_skeleton_requires_concrete_values\"}}},\"mode\":\"" + mode + "\",\"predicate_pattern\":{\"args\":[{\"kind\":\"var\",\"name\":\"${symbol}\"},{\"kind\":\"const\",\"sort\":{\"kind\":\"primitive\",\"name\":\"Ref\"},\"value\":null}],\"kind\":\"atomic\",\"name\":\"neq\"}}],\"sugar_name\":\"junit5\",\"target_language\":\"java\"},\"critical\":false,\"kind\":\"sugar\",\"protocol_versions\":[\"pep/1.7.0\"],\"provenance_cid\":\"blake3-512:0\",\"schemaVersion\":\"1\",\"version\":\"1.0.0\"}}";
    }

    private static String commentSugar() {
        return "{\"header\":{\"cid\":\"java-function-comment\",\"content\":{\"entries\":[{\"emission_template\":{\"kind\":\"verbatim\",\"surface_locator\":\"comment:above\",\"template\":\"// ${contract_role}: ${formula_pretty_print}\"},\"loss_record_contribution\":{\"form\":\"literal\",\"value\":{\"structural_divergence\":{\"args\":[],\"kind\":\"atomic\",\"name\":\"machine_uncheckable_prose\"}}},\"predicate_pattern\":{\"args\":[],\"kind\":\"atomic\",\"name\":\"${any_formula}\"}}],\"sugar_name\":\"function-comment\",\"target_language\":\"java\"},\"critical\":false,\"kind\":\"sugar\",\"protocol_versions\":[\"pep/1.7.0\"],\"provenance_cid\":\"blake3-512:0\",\"schemaVersion\":\"1\",\"version\":\"1.0.0\"}}";
    }

    private static String validCommentSugar() {
        return commentSugar().replace(
            "\"cid\":\"java-function-comment\"",
            "\"cid\":\"blake3-512:574800417e6f4f57e561dbe9c437adc691b2cd2369d964cbc329348cb715b161f3b38f6f7ccfd41537d033741488c081ec01b6f7cb3f04ba724b7003fa05a7b6\""
        );
    }

    private static TransportedOperation transportedOperation() {
        return new TransportedOperation(
            cid("a"),
            cid("b"),
            cid("c"),
            "skip",
            cid("d"),
            cid("e"),
            java.util.List.of(0),
            com.provekit.ir.Jcs.array(com.provekit.ir.Jcs.object(
                "kind", com.provekit.ir.Jcs.string("var"),
                "name", com.provekit.ir.Jcs.string("x")
            )),
            null,
            cid("f"),
            cid("0"),
            "concept:skip",
            "java"
        );
    }

    private static String conceptPayloadLine(String source) {
        for (String line : source.split("\\R")) {
            String stripped = line.strip();
            if (stripped.startsWith("// provekit-concept: ")) {
                return stripped.substring("// provekit-concept: ".length());
            }
        }
        throw new AssertionError("missing concept payload line:\n" + source);
    }

    private static String namedTermTree(String conceptName, String operationKind, String shapeCid) {
        return "{\"conceptName\":\"" + conceptName
            + "\",\"operationKind\":\"" + operationKind
            + "\",\"shapeCid\":\"" + shapeCid
            + "\",\"args\":[{\"conceptName\":\"concept:identity\",\"operationKind\":\"identity\",\"shapeCid\":\""
            + cid("3")
            + "\",\"args\":[]}]}";
    }

    private static String carrierLossRecordJson(String contribution) {
        return com.provekit.ir.Jcs.encode(com.provekit.ir.Jcs.object(
            contribution, com.provekit.ir.Jcs.object(
                "args", com.provekit.ir.Jcs.array(),
                "head", com.provekit.ir.Jcs.string("atomic"),
                "name", com.provekit.ir.Jcs.string(contribution)
            )
        ));
    }

    private static String cid(String ch) {
        return "blake3-512:" + ch.repeat(128);
    }

    private static String sugar(String cid, String name, String locator, String template, String loss) {
        return "{\"header\":{\"cid\":\"" + cid + "\",\"content\":{\"entries\":[{\"emission_template\":{\"kind\":\"verbatim\",\"surface_locator\":\"" + locator + "\",\"template\":\"" + template + "\"},\"loss_record_contribution\":{\"form\":\"literal\",\"value\":" + loss + "},\"predicate_pattern\":{\"args\":[{\"kind\":\"var\",\"name\":\"${symbol}\"},{\"kind\":\"const\",\"sort\":{\"kind\":\"primitive\",\"name\":\"Ref\"},\"value\":null}],\"kind\":\"atomic\",\"name\":\"neq\"}}],\"sugar_name\":\"" + name + "\",\"target_language\":\"java\"},\"critical\":false,\"kind\":\"sugar\",\"protocol_versions\":[\"pep/1.7.0\"],\"provenance_cid\":\"blake3-512:0\",\"schemaVersion\":\"1\",\"version\":\"1.0.0\"}}";
    }

    private static com.provekit.ir.Jcs.Json numericPredicate(String op, String symbol, long value) {
        return com.provekit.ir.Jcs.object(
            "args", com.provekit.ir.Jcs.array(
                com.provekit.ir.Jcs.object("kind", com.provekit.ir.Jcs.string("var"), "name", com.provekit.ir.Jcs.string(symbol)),
                com.provekit.ir.Jcs.object(
                    "kind", com.provekit.ir.Jcs.string("const"),
                    "sort", com.provekit.ir.Jcs.object("kind", com.provekit.ir.Jcs.string("primitive"), "name", com.provekit.ir.Jcs.string("Int")),
                    "value", new com.provekit.ir.Jcs.Num(value)
                )
            ),
            "kind", com.provekit.ir.Jcs.string("atomic"),
            "name", com.provekit.ir.Jcs.string(op)
        );
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
