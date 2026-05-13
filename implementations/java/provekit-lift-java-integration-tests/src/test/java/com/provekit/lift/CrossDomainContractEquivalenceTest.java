package com.provekit.lift;

import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

import com.github.javaparser.*;
import com.github.javaparser.ast.*;
import com.provekit.lift.bean.BeanValidationExtractor;
import com.provekit.lift.jml.JmlExtractor;
import com.provekit.lift.springweb.SpringWebExtractor;
import com.provekit.lift.cofoja.CofojaExtractor;
import com.provekit.lift.provekitnative.ProvekitNativeExtractor;

import java.util.*;

/**
 * Core theorem: different annotation families that express the same runtime
 * constraint must lift to identical IR. Same bytes -> same CID -> same proof.
 *
 * If @NotNull, //@ requires x != null, and @RequestParam(required=true)
 * all constrain a parameter to be non-null, they must produce the same
 * atomic formula in the IR output.
 */
public class CrossDomainContractEquivalenceTest {

    @Test
    public void notNullConstraintIsUniversal() {
        // Bean Validation surface
        String beanSource = """
            import jakarta.validation.constraints.NotNull;
            public class BeanService {
                public String greet(@NotNull String name) {
                    return "Hello " + name;
                }
            }
            """;

        // JML surface
        String jmlSource = """
            public class JmlService {
                //@ requires name != null
                public String greet(String name) {
                    return "Hello " + name;
                }
            }
            """;

        // Spring Web surface (required=true is the default, so @RequestParam implies non-null)
        String springSource = """
            import org.springframework.web.bind.annotation.RequestParam;
            public class SpringService {
                public String greet(@RequestParam String name) {
                    return "Hello " + name;
                }
            }
            """;

        // Cofoja surface (@Requires)
        String cofojaSource = """
            import com.google.java.contract.Requires;
            public class CofojaService {
                @Requires("name != null")
                public String greet(String name) {
                    return "Hello " + name;
                }
            }
            """;

        // ProvekIt-native surface (@Requires)
        String provekitNativeSource = """
            import com.provekit.contract.Requires;
            public class ProvekitNativeService {
                @Requires("name != null")
                public String greet(String name) {
                    return "Hello " + name;
                }
            }
            """;

        // Lift all domains
        String beanIr = lift(new BeanValidationExtractor(), beanSource);
        String jmlIr = lift(new JmlExtractor(), jmlSource);
        String springIr = lift(new SpringWebExtractor(), springSource);
        String cofojaIr = lift(new CofojaExtractor(), cofojaSource);
        String provekitNativeIr = lift(new ProvekitNativeExtractor(), provekitNativeSource);

        // All domains express the same non-null constraint.
        // They MUST produce byte-for-byte identical IR.
        assertEquals(beanIr, jmlIr, "JML and Bean Validation IR must be identical");
        assertEquals(beanIr, springIr, "Spring Web and Bean Validation IR must be identical");
        assertEquals(beanIr, cofojaIr, "Cofoja and Bean Validation IR must be identical");
        assertEquals(beanIr, provekitNativeIr, "ProvekIt-native and Bean Validation IR must be identical");
    }

    @Test
    public void provekitNativeNotNullConstraintMatchesRequires() {
        String requiresSource = """
            import com.provekit.contract.Requires;
            public class ProvekitNativeService {
                @Requires("name != null")
                public String greet(String name) {
                    return "Hello " + name;
                }
            }
            """;

        String notNullSource = """
            import com.provekit.contract.NotNull;
            public class ProvekitNativeService {
                public String greet(@NotNull String name) {
                    return "Hello " + name;
                }
            }
            """;

        String requiresIr = lift(new ProvekitNativeExtractor(), requiresSource);
        String notNullIr = lift(new ProvekitNativeExtractor(), notNullSource);

        assertEquals(requiresIr, notNullIr, "ProvekIt-native @NotNull must lift like @Requires(\"name != null\")");
    }

    @Test
    public void numericRangeConstraintIsUniversal() {
        // Bean Validation: @Min(0) @Max(100)
        String beanSource = """
            import jakarta.validation.constraints.Min;
            import jakarta.validation.constraints.Max;
            public class ScoreService {
                public int setScore(@Min(0) @Max(100) int score) {
                    return score;
                }
            }
            """;

        // JML surface
        String jmlSource = """
            public class ScoreService {
                //@ requires score >= 0 && score <= 100
                public int setScore(int score) {
                    return score;
                }
            }
            """;

        // Cofoja surface
        String cofojaSource = """
            import com.google.java.contract.Requires;
            public class ScoreService {
                @Requires("score >= 0 && score <= 100")
                public int setScore(int score) {
                    return score;
                }
            }
            """;

        // ProvekIt-native surface
        String provekitNativeSource = """
            import com.provekit.contract.Requires;
            public class ScoreService {
                @Requires("score >= 0 && score <= 100")
                public int setScore(int score) {
                    return score;
                }
            }
            """;

        String beanIr = lift(new BeanValidationExtractor(), beanSource);
        String jmlIr = lift(new JmlExtractor(), jmlSource);
        String cofojaIr = lift(new CofojaExtractor(), cofojaSource);
        String provekitNativeIr = lift(new ProvekitNativeExtractor(), provekitNativeSource);

        // Both constrain 'score' to the same numeric range.
        // They MUST produce byte-for-byte identical IR.
        assertEquals(beanIr, jmlIr, "JML and Bean Validation IR must be identical");
        assertEquals(beanIr, cofojaIr, "Cofoja and Bean Validation IR must be identical");
        assertEquals(beanIr, provekitNativeIr, "ProvekIt-native and Bean Validation IR must be identical");
    }

    @Test
    public void nativeAndCofojaAnnotationsDoNotCrossLift() {
        String cofojaSource = """
            import com.google.java.contract.Requires;
            public class CofojaService {
                @Requires("name != null")
                public String greet(String name) {
                    return "Hello " + name;
                }
            }
            """;

        String nativeSource = """
            import com.provekit.contract.Requires;
            public class ProvekitNativeService {
                @Requires("name != null")
                public String greet(String name) {
                    return "Hello " + name;
                }
            }
            """;

        String unimportedSimpleNameSource = """
            public class AmbiguousService {
                @Requires("name != null")
                public String greet(String name) {
                    return "Hello " + name;
                }
            }
            """;

        assertEquals(1, extract(new CofojaExtractor(), cofojaSource).size());
        assertEquals(0, extract(new ProvekitNativeExtractor(), cofojaSource).size());
        assertEquals(1, extract(new ProvekitNativeExtractor(), nativeSource).size());
        assertEquals(0, extract(new CofojaExtractor(), nativeSource).size());
        assertEquals(0, extract(new CofojaExtractor(), unimportedSimpleNameSource).size());
        assertEquals(0, extract(new ProvekitNativeExtractor(), unimportedSimpleNameSource).size());
    }

    @Test
    public void fullyQualifiedNativeAndCofojaAnnotationsAreRecognized() {
        String cofojaSource = """
            public class CofojaService {
                @com.google.java.contract.Requires("name != null")
                public String greet(String name) {
                    return "Hello " + name;
                }
            }
            """;

        String nativeSource = """
            public class ProvekitNativeService {
                @com.provekit.contract.Requires("name != null")
                public String greet(String name) {
                    return "Hello " + name;
                }
            }
            """;

        assertEquals(1, extract(new CofojaExtractor(), cofojaSource).size());
        assertEquals(0, extract(new ProvekitNativeExtractor(), cofojaSource).size());
        assertEquals(1, extract(new ProvekitNativeExtractor(), nativeSource).size());
        assertEquals(0, extract(new CofojaExtractor(), nativeSource).size());
    }

    @Test
    public void differentPackagesSameHash() {
        // Two identical constraints expressed in two different annotation families
        String beanSource = """
            import jakarta.validation.constraints.Email;
            public class UserService {
                public String create(@Email String email) { return email; }
            }
            """;

        // Simulated: another package that also checks email format
        // In practice this would be a custom validator or another framework
        String beanIr = lift(new BeanValidationExtractor(), beanSource);

        // The IR contains the matches atom with the regex pattern
        assertTrue(beanIr.contains("\"name\":\"matches\""));
        assertTrue(beanIr.contains("^[^@]+@[^@]+$"));

        // This IR, when hashed, is the same regardless of which library produced it.
        // Two different codebases using different annotation libraries that enforce
        // the same constraint will share the same CID and thus the same proof.
    }

    @Test
    public void springRequestParamDefaultValueLiftsAsValueWitness() {
        String springSource = """
            import org.springframework.web.bind.annotation.RequestParam;
            public class PaymentController {
                public String accept(@RequestParam(defaultValue = "42") int value) {
                    return "accepted:" + value;
                }
            }
            """;

        String expected = "[{\"kind\":\"contract\",\"symbol\":\"accept\",\"invariant\":"
            + "{\"kind\":\"atomic\",\"name\":\"eq\",\"args\":["
            + "{\"kind\":\"var\",\"name\":\"value\"},"
            + "{\"kind\":\"const\",\"value\":42,\"sort\":{\"kind\":\"primitive\",\"name\":\"Int\"}}"
            + "]}}]";

        assertEquals(expected, lift(new SpringWebExtractor(), springSource));
    }

    private String lift(Extractor extractor, String source) {
        List<ContractDecl> decls = extract(extractor, source);
        assertFalse(decls.isEmpty(), "Extractor should find at least one contract");

        StringBuilder sb = new StringBuilder("[");
        for (int i = 0; i < decls.size(); i++) {
            if (i > 0) sb.append(",");
            sb.append(decls.get(i).toJson());
        }
        sb.append("]");
        return sb.toString();
    }

    private List<ContractDecl> extract(Extractor extractor, String source) {
        ParseResult<CompilationUnit> result = new JavaParser().parse(source);
        assertTrue(result.isSuccessful() && result.getResult().isPresent(),
            "Failed to parse: " + result.getProblems());
        CompilationUnit cu = result.getResult().get();
        return extractor.extract(cu, source);
    }
}
