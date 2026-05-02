# ProvekIt Java Lifter

SLF4J-style architecture: one core facade + one binding JAR per annotation package.

## The Core Insight

> **Same IR → same hash → same proof → shared across domains automatically.**

A `@NotNull` in Jakarta Bean Validation, a `//@ requires x != null` in JML, and a `@Column(nullable=false)` in JPA all lift to the same atomic formula `neq(x, null)`. Same bytes, same CID, same verification. The domain boundary dissolves because the hash doesn't care where the annotation came from.

## Architecture

```
provekit-lift-java-core              (facade — RPC, walking, IR emission)
├── provekit-lift-java-bean-validation   (@NotNull, @Email, @Min, @Size...)
├── provekit-lift-java-jml               (//@ requires, //@ ensures)
├── provekit-lift-java-cofoja            (@Requires, @Ensures)
├── provekit-lift-java-spring-web        (@GetMapping, @RequestParam, @ResponseStatus)
├── provekit-lift-java-spring-security   (@PreAuthorize, @Secured)
├── provekit-lift-java-swagger           (@ApiResponse, @Schema, @ApiParam)
├── provekit-lift-java-jackson           (@JsonProperty, @JsonIgnore)
├── provekit-lift-java-jpa               (@Entity, @Id, @Column, @ManyToOne)
└── provekit-lift-java-hibernate         (@Immutable, @NaturalId, @Where, @Formula)
```

You install the core + whichever bindings match the annotation libraries you use.

## Supported Annotation Packages

| Binding JAR | Annotation Library | What gets lifted |
|---|---|---|
| `provekit-lift-java-bean-validation` | Jakarta Bean Validation | `@NotNull`, `@NotEmpty`, `@NotBlank`, `@Email`, `@Min`, `@Max`, `@Size`, `@Pattern`, `@Positive`, `@Negative`, `@PositiveOrZero`, `@NegativeOrZero`, `@AssertTrue`, `@AssertFalse`, `@DecimalMin`, `@DecimalMax`, `@Digits`, `@Future`, `@Past` |
| `provekit-lift-java-jml` | JML (Java Modeling Language) | `//@ requires <expr>`, `//@ ensures <expr>`, `//@ invariant <expr>` |
| `provekit-lift-java-cofoja` | Cofoja (Contracts for Java) | `@Requires("<expr>")`, `@Ensures("<expr>")`, `@Invariant("<expr>")` |
| `provekit-lift-java-spring-web` | Spring Web / Spring MVC | `@RequestMapping`, `@GetMapping`, `@PostMapping`, `@PutMapping`, `@DeleteMapping`, `@PatchMapping`, `@RequestParam`, `@PathVariable`, `@RequestBody`, `@RequestHeader`, `@ResponseStatus` |
| `provekit-lift-java-spring-security` | Spring Security | `@PreAuthorize`, `@PostAuthorize`, `@Secured`, `@RolesAllowed` |
| `provekit-lift-java-swagger` | Swagger / OpenAPI | `@ApiOperation`, `@ApiResponse`, `@ApiResponses`, `@ApiParam`, `@Schema` |
| `provekit-lift-java-jackson` | Jackson (JSON serialization) | `@JsonProperty`, `@JsonIgnore`, `@JsonFormat` |
| `provekit-lift-java-jpa` | JPA (Java Persistence API) | `@Entity`, `@Id`, `@Column`, `@ManyToOne`, `@OneToOne`, `@NotNull` |
| `provekit-lift-java-hibernate` | Hibernate (ORM extensions) | `@Immutable`, `@NaturalId`, `@Where`, `@Check`, `@Formula`, `@Filter`, `@FilterDef`, `@BatchSize`, `@Fetch`, `@LazyCollection`, `@JoinFormula`, `@Subselect`, `@DynamicUpdate`, `@DynamicInsert`, `@SelectBeforeUpdate`, `@OptimisticLocking`, `@DiscriminatorFormula`, `@Synchronize`, `@Type`, `@GenericGenerator`, `@SQLDelete`, `@SQLInsert`, `@SQLUpdate`, `@RowId` |

## How it works

1. You add annotation libraries to your project:
   ```xml
   <dependency>
       <groupId>jakarta.validation</groupId>
       <artifactId>jakarta.validation-api</artifactId>
   </dependency>
   <dependency>
       <groupId>org.springframework.boot</groupId>
       <artifactId>spring-boot-starter-web</artifactId>
   </dependency>
   ```
2. You annotate your code with them
3. `provekit mint` launches `provekit-lift-java-core`
4. The core uses `ServiceLoader` to discover which binding JARs are on the classpath
5. Each binding scans the AST for its annotation family and emits IR
6. All IR is collected, bundled, and minted as a `.proof`

## Cross-domain contract sharing

Because all bindings emit the same canonical IR, contracts from different annotation families that express the same constraint share the same hash:

```java
// Bean Validation
@NotNull
public String name;

// JML
//@ requires name != null
public String getName() { ... }

// JPA
@Column(nullable=false)
public String name;
```

All three lift to:
```json
{"kind":"atomic","name":"neq","args":[{"kind":"var","name":"name"},{"kind":"const","value":null,"sort":{"kind":"primitive","name":"Ref"}}]}
```

Same bytes → same CID → same verification proof. Once one is verified, all are verified.

## Build

```bash
cd implementations/java
mvn package
```

Produces:
- `provekit-lift-java-core/target/provekit-lift-java-core-0.1.0-shaded.jar`
- Individual binding JARs in each module's `target/` directory

## Usage

### RPC plugin mode

Put the core + bindings on the classpath:

```bash
java -cp "core.jar:bean-validation.jar:spring-web.jar:hibernate.jar" com.provekit.lift.Main --rpc
```

The core discovers bindings via `ServiceLoader`. No config needed.

### Manifest

`.provekit/lift/java/manifest.toml`:

```toml
name = "java"
version = "1.0.0"
protocol_version = "provekit-lift/1"
command = ["java", "-cp", "provekit-lift-java-core-0.1.0-shaded.jar:provekit-lift-java-bean-validation-0.1.0.jar:provekit-lift-java-spring-web-0.1.0.jar", "com.provekit.lift.Main", "--rpc"]

[capabilities]
authoring_surfaces = ["java-bean-validation", "java-jml", "java-cofoja", "java-spring-web", "java-spring-security", "java-swagger", "java-jackson", "java-jpa", "java-hibernate"]
ir_version = "v1.1.0"
```

You only need the binding JARs for the annotation libraries your project uses.

## Java IR Kit

When annotations aren't enough, construct IR directly:

```java
import com.provekit.ir.*;

Term x = Term.var_("x", Sort.Int);
Term zero = Term.const_(0, Sort.Int);
Formula post = Formula.atomic("gte", x, zero);

IrDocument doc = IrDocument.builder()
    .contract("abs", null, post)
    .build();

String json = doc.toJson();
```

The IR kit provides Java-native builder classes for `Sort`, `Term`, `Formula`, `Declaration`, and `IrDocument` with full JSON serialization matching the v1.1.0 grammar.

## Adding a new binding

1. Create a new Maven module depending on `provekit-lift-java-core`
2. Implement `com.provekit.lift.Extractor`
3. Register it in `META-INF/services/com.provekit.lift.Extractor`
4. `mvn install`
5. Add your JAR to the classpath

No changes to the core. No recompilation. Just like adding a new SLF4J binding.

## Example: Hibernate

```java
@Entity
@Immutable
@Where(clause = "active = true")
@Check(constraints = "price > 0")
public class Product {
    @NaturalId
    private String sku;

    @Formula("price * 0.9")
    private BigDecimal discountPrice;

    @BatchSize(size = 10)
    @Fetch(FetchMode.SUBSELECT)
    private List<Order> orders;
}
```

Lifts to IR invariants:
- `immutable("Product")`
- `where_clause("Product", "active = true")`
- `db_check("Product", "price > 0")`
- `natural_id("Product", "sku", false)`
- `formula("Product", "discountPrice", "price * 0.9")`
- `batch_size("Product", "orders", 10)`
- `fetch_mode("Product", "orders", "SUBSELECT")`

All invisible to Java's type checker, all verifiable by ProvekIt.
