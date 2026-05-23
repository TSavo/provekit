package com.provekit.emit.junit;

import java.util.Optional;

import com.provekit.ir.Jcs;

/**
 * INLINE predicate -> JUnit5 assertion mapping.
 *
 * <p>This is the heart of the emitter and the whole point of PR-6's reframe:
 * the fact that {@code concept:eq} spells as JUnit5's {@code assertEquals}
 * is JAVA FRAMEWORK KNOWLEDGE, written here in java code. It is NOT substrate
 * data. There is no catalog memento family for this mapping and no catalog
 * read for the framework spelling. (PR-5 tried to externalize it into a
 * {@code JUnitAssertionTemplateMemento} family; that was closed as an
 * architectural mistake.)
 *
 * <p>The mapping is the inverse of {@code provekit-lift-java-junit}'s
 * {@code JUnitExtractor}, which recognizes these same assertions and lifts
 * them back to neutral predicates. If the two ever want to share the table
 * it becomes a normal java module dependency, never a substrate catalog
 * memento.
 *
 * <p>Supported neutral predicates (catalog spelling, with the {@code concept:}
 * prefix stripped by {@link JUnitEmitter}):
 * <ul>
 *   <li>{@code eq(a, b)}            -> {@code assertEquals(a, b)}</li>
 *   <li>{@code ne(a, b)}            -> {@code assertNotEquals(a, b)}</li>
 *   <li>{@code lt(a, b)}            -> {@code assertTrue(a < b)}</li>
 *   <li>{@code gt(a, b)}            -> {@code assertTrue(a > b)}</li>
 *   <li>{@code le(a, b)}            -> {@code assertTrue(a <= b)}</li>
 *   <li>{@code ge(a, b)}            -> {@code assertTrue(a >= b)}</li>
 *   <li>{@code option-is-some(x)}   -> {@code assertNotNull(x)}</li>
 *   <li>{@code fallible-err(x)}     -> {@code assertThrows(Exception.class, () -> x)}</li>
 * </ul>
 */
final class PredicateAssertionTable {
    private PredicateAssertionTable() {}

    /**
     * Render a single neutral predicate term as one JUnit5 assertion statement
     * (no trailing newline, no indentation). The term is the catalog-form
     * op node: {@code {"kind":"op"|"atomic","name":"concept:eq","args":[...]}}.
     *
     * <p>Returns {@code Optional.empty()} if the predicate head is not in this
     * kit's table or the arity is wrong — substrate-honest: an unsupported
     * predicate is NOT silently dropped into a passing assertion; the caller
     * surfaces it as an unemitted gap.
     */
    static Optional<String> render(Jcs.Obj predicate) {
        String head = headOf(predicate);
        if (head == null) return Optional.empty();
        java.util.List<Jcs.Json> args = argsOf(predicate);

        switch (head) {
            case "eq":
                return binary(args, (a, b) -> "assertEquals(" + a + ", " + b + ");");
            case "ne":
            case "neq": // JUnitExtractor's internal spelling; accept both
                return binary(args, (a, b) -> "assertNotEquals(" + a + ", " + b + ");");
            case "lt":
                return binary(args, (a, b) -> "assertTrue(" + a + " < " + b + ");");
            case "gt":
                return binary(args, (a, b) -> "assertTrue(" + a + " > " + b + ");");
            case "le":
            case "lte":
                return binary(args, (a, b) -> "assertTrue(" + a + " <= " + b + ");");
            case "ge":
            case "gte":
                return binary(args, (a, b) -> "assertTrue(" + a + " >= " + b + ");");
            case "option-is-some":
            case "not-null":
                return unary(args, x -> "assertNotNull(" + x + ");");
            case "option-is-none":
                return unary(args, x -> "assertNull(" + x + ");");
            case "fallible-err":
                // The predicate's single argument is the fallible expression
                // whose evaluation is asserted to throw. We wrap it in a
                // throwing-supplier lambda; the catch type is the broadest
                // checked surface (Exception) since the neutral concept does
                // not pin a specific java exception class.
                return unary(args, x ->
                    "assertThrows(Exception.class, () -> { " + x + "; });");
            default:
                return Optional.empty();
        }
    }

    /** True if this kit can emit an assertion for the given predicate head. */
    static boolean supports(String head) {
        if (head == null) return false;
        switch (head) {
            case "eq":
            case "ne":
            case "neq":
            case "lt":
            case "gt":
            case "le":
            case "lte":
            case "ge":
            case "gte":
            case "option-is-some":
            case "not-null":
            case "option-is-none":
            case "fallible-err":
                return true;
            default:
                return false;
        }
    }

    /**
     * The predicate head with the {@code concept:} prefix stripped, or
     * {@code null} if the node is malformed. Accepts both the catalog form
     * ({@code kind:"op"}, {@code name:"concept:eq"}) and the harvester's
     * internal form ({@code kind:"atomic"}, {@code name:"eq"}).
     */
    static String headOf(Jcs.Obj predicate) {
        String name = predicate.stringFieldOrNull("name");
        if (name == null || name.isBlank()) return null;
        return name.startsWith("concept:") ? name.substring("concept:".length()) : name;
    }

    private static java.util.List<Jcs.Json> argsOf(Jcs.Obj predicate) {
        Jcs.Json args = predicate.get("args");
        if (args instanceof Jcs.Arr arr) return arr.values();
        return java.util.List.of();
    }

    private interface Binary {
        String render(String a, String b);
    }

    private interface Unary {
        String render(String x);
    }

    private static Optional<String> binary(java.util.List<Jcs.Json> args, Binary f) {
        if (args.size() != 2) return Optional.empty();
        Optional<String> a = TermRenderer.render(args.get(0));
        Optional<String> b = TermRenderer.render(args.get(1));
        if (a.isEmpty() || b.isEmpty()) return Optional.empty();
        return Optional.of(f.render(a.get(), b.get()));
    }

    private static Optional<String> unary(java.util.List<Jcs.Json> args, Unary f) {
        if (args.size() != 1) return Optional.empty();
        Optional<String> x = TermRenderer.render(args.get(0));
        if (x.isEmpty()) return Optional.empty();
        return Optional.of(f.render(x.get()));
    }
}
