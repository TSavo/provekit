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
}
