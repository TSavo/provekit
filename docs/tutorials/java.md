# Tutorial: Java / JVM

> **Status:** kit shipping (v1.4.1). Lift adapters shipping: Bean Validation, JML, Spring Web, Cofoja, plus bindings for Spring Security, Swagger, Jackson, JPA, Hibernate. Embedded verifier and LSP plugin planned. Verification via the Rust CLI.

A walkthrough for Java / JVM developers. By the end you have a `.proof` catalog lifted from existing `@NotNull`, `@Email`, `@Min`, `//@ requires`, `@RequestParam` annotations (across Bean Validation, JML, Spring, and Cofoja sources, all canonicalized to the same IR).

## 1. What you'll have at the end

- A `.proof` file alongside your Maven artifact.
- Mementos derived from existing JVM annotations across multiple annotation idioms (without rewriting any code).
- Cross-domain integration: `@NotNull`, `//@ requires x != null`, and `@RequestParam(required=true)` produce identical IR; the Bean Validation adapter and the JML adapter and the Spring Web adapter agree by hash.

## 2. Prerequisites

- JDK 17+.
- Maven 3.9+.
- Rust toolchain on `PATH` (verifier subprocess).
- Z3 on `PATH` (Tier 3 only).

## 3. Install

```bash
# the canonical verifier (Rust CLI)
cargo install provekit
provekit verify-protocol

# the Java kit (multi-module Maven, ServiceLoader-discovered)
cd implementations/java && mvn install
```

The Java kit uses an SLF4J-style architecture: `provekit-lift-java-core` (facade) plus per-annotation binding JARs discovered via `java.util.ServiceLoader`. Add the bindings you need to your `pom.xml`.

## 4. Lift your first contract

If your codebase already uses Bean Validation:

```java
public class User {
    @NotNull @Email
    private String email;

    @Min(0) @Max(150)
    private int age;
}
```

Or JML:

```java
//@ requires email != null && email.matches("^[^@]+@[^@]+\\.[^@]+$");
//@ requires age >= 0 && age <= 150;
public void registerUser(String email, int age) { ... }
```

Or Spring Web:

```java
@PostMapping("/users")
public User register(
    @RequestParam(required = true) String email,
    @RequestParam(required = true) @Min(0) @Max(150) int age
) { ... }
```

All three lift to byte-identical IR for equivalent constraints. Run the lifter:

```bash
mvn provekit:lift
```

Output: `target/.proof`.

## 5. Verify

```bash
provekit prove
```

Same handshake, same discharge shape as the [Rust tutorial step 4](rust.md#step-4-verify).

## 6. Wire your IDE and CI

- **IDE:** the JVM LSP plugin is planned for v1.2. Until then, no in-editor squigglies.
- **CI:** see [docs/how-to/ci-integration/github-actions.md](../how-to/ci-integration/github-actions.md).

## Cross-domain equivalence

The Java kit's load-bearing claim is that semantically-equivalent constraints across annotation idioms produce byte-identical IR. Integration tests in `implementations/java/` prove this for `@NotNull` ↔ `//@ requires x != null` ↔ `@RequestParam(required=true)`, and for `@Min(0) @Max(100)` ↔ `//@ requires score >= 0 && score <= 100`.

This is what makes mixed-style codebases (Spring + JML + Bean Validation) participate in the same hash-equality handshake.

## What's next

- [docs/how-to/publishing-a-proof.md](../how-to/publishing-a-proof.md): ship the `.proof` alongside your Maven artifact.
- [docs/how-to/cross-domain-bridges.md](../how-to/cross-domain-bridges.md).
- [docs/reference/per-adapter-coverage.md](../reference/per-adapter-coverage.md): what each adapter sees and misses.
- [docs/explanation/thesis.md](../explanation/thesis.md).

---

*This tutorial is a stub. Contributions welcome (see [docs/contributing/overview.md](../contributing/overview.md). Known gaps: actual `mvn provekit:lift` plugin coordinates, end-to-end runnable example.*)
