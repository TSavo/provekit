package jakarta.validation.constraints;

import java.lang.annotation.Documented;
import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;

/**
 * Vendored stub of the Bean Validation (JSR-380) {@code @Pattern} constraint
 * annotation — the same declaration shipped in
 * {@code jakarta.validation:jakarta.validation-api}, tag {@code 3.0.2}.
 *
 * The annotation's contract: the annotated CharSequence must match the regular
 * expression {@link #regexp()}. This stub carries ONLY the {@code regexp}
 * element the kit walks; it is parsed (not executed) — the kit reads the
 * {@code @Pattern(regexp="…")} string literal from the annotation's AST node.
 */
@Target({ ElementType.METHOD, ElementType.FIELD, ElementType.PARAMETER })
@Retention(RetentionPolicy.RUNTIME)
@Documented
public @interface Pattern {
    String regexp();

    String message() default "{jakarta.validation.constraints.Pattern.message}";
}
