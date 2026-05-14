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

    private static String modeScopedBeanValidationSugar(String mode) {
        return "{\"header\":{\"cid\":\"java-bean-validation\",\"content\":{\"entries\":[{\"emission_template\":{\"kind\":\"verbatim\",\"surface_locator\":\"annotation:before-parameter\",\"template\":\"@NotNull\"},\"loss_record_contribution\":{\"form\":\"literal\",\"value\":{}},\"mode\":\"" + mode + "\",\"predicate_pattern\":{\"args\":[{\"kind\":\"var\",\"name\":\"${symbol}\"},{\"kind\":\"const\",\"sort\":{\"kind\":\"primitive\",\"name\":\"Ref\"},\"value\":null}],\"kind\":\"atomic\",\"name\":\"neq\"}}],\"sugar_name\":\"bean-validation\",\"target_language\":\"java\"},\"critical\":false,\"kind\":\"sugar\",\"protocol_versions\":[\"pep/1.7.0\"],\"provenance_cid\":\"blake3-512:0\",\"schemaVersion\":\"1\",\"version\":\"1.0.0\"}}";
    }

    private static String modeScopedJunitSugar(String mode) {
        return "{\"header\":{\"cid\":\"java-junit5\",\"content\":{\"entries\":[{\"emission_template\":{\"kind\":\"verbatim\",\"surface_locator\":\"witness:junit5-test\",\"template\":\"assertNotNull(${symbol});\"},\"loss_record_contribution\":{\"form\":\"literal\",\"value\":{\"domain_narrowing\":{\"args\":[],\"kind\":\"atomic\",\"name\":\"witness_requires_test_execution\"},\"structural_divergence\":{\"args\":[],\"kind\":\"atomic\",\"name\":\"witness_skeleton_requires_concrete_values\"}}},\"mode\":\"" + mode + "\",\"predicate_pattern\":{\"args\":[{\"kind\":\"var\",\"name\":\"${symbol}\"},{\"kind\":\"const\",\"sort\":{\"kind\":\"primitive\",\"name\":\"Ref\"},\"value\":null}],\"kind\":\"atomic\",\"name\":\"neq\"}}],\"sugar_name\":\"junit5\",\"target_language\":\"java\"},\"critical\":false,\"kind\":\"sugar\",\"protocol_versions\":[\"pep/1.7.0\"],\"provenance_cid\":\"blake3-512:0\",\"schemaVersion\":\"1\",\"version\":\"1.0.0\"}}";
    }

    private static String commentSugar() {
        return "{\"header\":{\"cid\":\"java-function-comment\",\"content\":{\"entries\":[{\"emission_template\":{\"kind\":\"verbatim\",\"surface_locator\":\"comment:above\",\"template\":\"// ${contract_role}: ${formula_pretty_print}\"},\"loss_record_contribution\":{\"form\":\"literal\",\"value\":{\"structural_divergence\":{\"args\":[],\"kind\":\"atomic\",\"name\":\"machine_uncheckable_prose\"}}},\"predicate_pattern\":{\"args\":[],\"kind\":\"atomic\",\"name\":\"${any_formula}\"}}],\"sugar_name\":\"function-comment\",\"target_language\":\"java\"},\"critical\":false,\"kind\":\"sugar\",\"protocol_versions\":[\"pep/1.7.0\"],\"provenance_cid\":\"blake3-512:0\",\"schemaVersion\":\"1\",\"version\":\"1.0.0\"}}";
    }

    private static String sugar(String cid, String name, String locator, String template, String loss) {
        return "{\"header\":{\"cid\":\"" + cid + "\",\"content\":{\"entries\":[{\"emission_template\":{\"kind\":\"verbatim\",\"surface_locator\":\"" + locator + "\",\"template\":\"" + template + "\"},\"loss_record_contribution\":{\"form\":\"literal\",\"value\":" + loss + "},\"predicate_pattern\":{\"args\":[{\"kind\":\"var\",\"name\":\"${symbol}\"},{\"kind\":\"const\",\"sort\":{\"kind\":\"primitive\",\"name\":\"Ref\"},\"value\":null}],\"kind\":\"atomic\",\"name\":\"neq\"}}],\"sugar_name\":\"" + name + "\",\"target_language\":\"java\"},\"critical\":false,\"kind\":\"sugar\",\"protocol_versions\":[\"pep/1.7.0\"],\"provenance_cid\":\"blake3-512:0\",\"schemaVersion\":\"1\",\"version\":\"1.0.0\"}}";
    }
}
