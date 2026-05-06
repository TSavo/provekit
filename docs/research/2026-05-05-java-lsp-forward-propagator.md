# Research: Java LSP plugin entry and ForwardPropagator wiring

## Scope

This note documents the current state of the Java kit in the Provekit monorepo, whether an LSP plugin exists, what AST infrastructure is available, and what the ForwardPropagator (#319) will need to build. The goal is to provide enough context for a low-context agent to implement #319 without further investigation.

## Findings

### LSP plugin: DOES NOT EXIST

There is no `provekit-lsp-java` directory, binary, or module anywhere in `implementations/java/`. No file matching `*lsp*`, `*Lsp*`, or `*forward*` exists in the Java kit tree.

The Java kit has **lift-plugin protocol** infrastructure (RPC handler in `Rpc.java`) but no **LSP protocol** infrastructure. These are distinct protocols:

- Lift protocol: `initialize` + `lift` (for minting self-contracts)
- LSP protocol: `initialize` + `parse` (for editor diagnostics)

### Existing RPC infrastructure: `provekit-java-self-contracts/src/main/java/com/provekit/selfcontracts/Rpc.java`

The Java kit already has an NDJSON-over-stdio RPC handler for the lift protocol. This is the closest existing pattern the ForwardPropagator can reuse for its JSON-RPC dispatch:

```java
// implementations/java/provekit-java-self-contracts/src/main/java/com/provekit/selfcontracts/Rpc.java:44-60
private static final Pattern ID_FIELD = Pattern.compile(
    "\"id\"\\s*:\\s*(\"[^\"]*\"|\\d+|null|true|false)");
private static final Pattern METHOD_FIELD = Pattern.compile(
    "\"method\"\\s*:\\s*\"([^\"]*)\"");

public static void run() throws IOException {
    BufferedReader in = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
    PrintStream out = System.out;

    String line;
    while ((line = in.readLine()) != null) {
        line = line.trim();
        if (line.isEmpty()) continue;

        String idRaw = matchOrNull(ID_FIELD, line);
        String method = matchOrNull(METHOD_FIELD, line);
```

The RPC handler uses hand-rolled regex-based JSON parsing (no Jackson dependency for the RPC layer). Responses are emitted as strict JSON strings. This pattern can be extended for LSP methods.

### Self-contracts module: `provekit-java-self-contracts/`

Standard Maven layout under `implementations/java/provekit-java-self-contracts/`:

```
provekit-java-self-contracts/
  src/main/java/com/provekit/selfcontracts/
    Main.java          # CLI entry point
    Rpc.java           # NDJSON RPC handler (lift protocol)
    Orchestrator.java  # Slab orchestration
    Slab.java          # Contract slab interface
    JavaKitInvariants.java
```

### AST infrastructure: JavaParser (Maven dependency)

The Java kit uses `javaparser-symbol-solver-core` (v3.26.4) as a Maven dependency in the parent POM. This is the standard Java AST parsing library. Any LSP plugin will use JavaParser to:

1. Parse Java source into a `CompilationUnit`
2. Walk the AST for `// @provekit-contract` annotations or Javadoc tags
3. Extract method signatures, parameter types, return types
4. Lift to Provekit IR (`Declaration`, `ContractDecl`)

The `provekit-ir` module provides the Java IR types:

```
provekit-ir/src/main/java/com/provekit/ir/
  Sort.java
  IrDocument.java
  Declaration.java
  BridgeDeclarationV14.java
  BridgeHeaderV14.java
  BridgeEnvelope.java
  BridgeMetadataV14.java
```

### Maven structure

The Java kit is a multi-module Maven project with 13 modules:

| Module | Purpose |
|--------|---------|
| `provekit-ir` | IR type definitions (Sort, Declaration, BridgeV14) |
| `provekit-claim-envelope` | Claim envelope types |
| `provekit-java-self-contracts` | Self-contracts minting + RPC handler |
| `provekit-lift-java-core` | Core lift infrastructure |
| `provekit-lift-java-bean-validation` | Bean validation lift |
| `provekit-lift-java-jml` | JML annotation lift |
| `provekit-lift-java-cofoja` | Cofoja contract lift |
| `provekit-lift-java-spring-web` | Spring Web lift |
| `provekit-lift-java-spring-security` | Spring Security lift |
| `provekit-lift-java-swagger` | Swagger/OpenAPI lift |
| `provekit-lift-java-jackson` | Jackson annotation lift |
| `provekit-lift-java-jpa` | JPA annotation lift |
| `provekit-lift-java-hibernate` | Hibernate annotation lift |

Java 17 is the target (`maven.compiler.source=17`).

## Conventions

- Maven multi-module layout under `implementations/java/` with parent POM.
- Package naming: `com.provekit.<module-name>` (e.g., `com.provekit.selfcontracts`, `com.provekit.ir`).
- No `provekit-lsp-java` module exists yet; all lift modules follow the `provekit-lift-java-*` naming convention.
- The lift protocol RPC handler (`Rpc.java`) uses hand-rolled regex JSON parsing, not Jackson, to minimize dependencies in the RPC path.
- Build: `mvn package` from `implementations/java/`. No separate build step for the LSP (since it does not exist yet).
- Test runner: JUnit via Maven Surefire.

## Open questions

1. **Should the LSP plugin be a new Maven module `provekit-lsp-java/` or added to an existing module?**

   Proposed: new module `provekit-lsp-java/` following the existing naming convention. This keeps the LSP surface separate from lift modules and mirrors the Go kit's `cmd/provekit-lsp-go/` pattern. The module would depend on `provekit-ir` and `javaparser-symbol-solver-core`.

2. **What should the binary entry point be?**

   Proposed: `provekit-lsp-java/src/main/java/com/provekit/lsp/Main.java` with a `main()` method that runs the NDJSON loop. The Maven `appassembler-maven-plugin` or `maven-shade-plugin` can produce an executable JAR. The ForwardPropagator will spawn it as `java -jar provekit-lsp-java-0.1.0.jar`.

3. **Protocol shape mismatch (#298):** The lift protocol uses `initialize` + `lift` while LSP uses `initialize` + `parse`. Should the Java LSP support both in one binary, or should they be separate?

   Proposed: separate binaries. The lift protocol binary already exists (`Rpc.java` in self-contracts). The LSP binary should only support `initialize`, `parse`, and `shutdown`. This matches the Go kit's separation (`cmd/provekit-lsp-go/` vs `cmd/mint-go-self-contracts/`).

4. **Does the Java LSP need to support annotation scanning beyond `// @provekit-contract`?**

   Proposed: start with line-comment annotation scanning (matching the PHP `lspd.php` pattern). Javadoc-based annotations and JSR-308 type annotations can be added as follow-ups. JavaParser supports both.
