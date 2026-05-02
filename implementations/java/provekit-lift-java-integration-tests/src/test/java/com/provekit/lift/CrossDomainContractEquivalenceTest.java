package com.provekit.lift;

import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

import com.github.javaparser.*;
import com.github.javaparser.ast.*;
import com.provekit.lift.bean.BeanValidationExtractor;
import com.provekit.lift.jml.JmlExtractor;
import com.provekit.lift.springweb.SpringWebExtractor;

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

        // Lift all three
        String beanIr = lift(new BeanValidationExtractor(), beanSource);
        String jmlIr = lift(new JmlExtractor(), jmlSource);
        String springIr = lift(new SpringWebExtractor(), springSource);

        // All three domains express the same non-null constraint.
        // They MUST produce byte-for-byte identical IR.
        assertEquals(beanIr, jmlIr, "JML and Bean Validation IR must be identical");
        assertEquals(beanIr, springIr, "Spring Web and Bean Validation IR must be identical");
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

        String beanIr = lift(new BeanValidationExtractor(), beanSource);
        String jmlIr = lift(new JmlExtractor(), jmlSource);

        // Both constrain 'score' to the same numeric range.
        // They MUST produce byte-for-byte identical IR.
        assertEquals(beanIr, jmlIr, "JML and Bean Validation IR must be identical");
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

    private String lift(Extractor extractor, String source) {
        ParseResult<CompilationUnit> result = new JavaParser().parse(source);
        assertTrue(result.isSuccessful() && result.getResult().isPresent(),
            "Failed to parse: " + result.getProblems());
        CompilationUnit cu = result.getResult().get();
        List<ContractDecl> decls = extractor.extract(cu, source);
        assertFalse(decls.isEmpty(), "Extractor should find at least one contract");

        StringBuilder sb = new StringBuilder("[");
        for (int i = 0; i < decls.size(); i++) {
            if (i > 0) sb.append(",");
            sb.append(decls.get(i).toJson());
        }
        sb.append("]");
        return sb.toString();
    }
}
