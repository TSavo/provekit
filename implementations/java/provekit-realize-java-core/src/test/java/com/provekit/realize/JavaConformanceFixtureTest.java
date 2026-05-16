package com.provekit.realize;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.junit.jupiter.api.Assertions.fail;

import com.provekit.ir.Jcs;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Duration;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.Set;
import java.util.concurrent.TimeUnit;
import java.util.stream.Stream;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

class JavaConformanceFixtureTest {
    private static final String TARGET_COMPILE_FAILURE = "target-compile-failure";
    private static final String TARGET_BEHAVIOR_DIVERGENCE = "target-behavior-divergence";
    private static final Set<String> REQUIRED_FIXTURES = Set.of(
        "hello_world",
        "recursive_factorial",
        "arithmetic_add",
        "control_flow_if",
        "transported_op_drop"
    );

    @Test
    void javaCarrierFixturesCompileAndRunAgainstOriginalBehavior(@TempDir Path tempDir) throws Exception {
        List<Fixture> fixtures = loadFixtures();

        assertEquals(5, fixtures.size(), "Java conformance fixture set must have N=5");
        assertEquals(REQUIRED_FIXTURES, fixtures.stream().map(Fixture::name).collect(java.util.stream.Collectors.toSet()));

        for (Fixture fixture : fixtures) {
            Path fixtureWork = tempDir.resolve(fixture.name());
            Files.createDirectories(fixtureWork);

            SugarRealizer.Realization emitted = fixture.emission().emit();
            Path emittedSource = writeEmittedSource(fixture, emitted.source(), fixtureWork.resolve("emitted"));
            compileEmittedSource(emittedSource);

            if ("carrier_comment".equals(fixture.behavior())) {
                assertCarrierCommentSurvives(fixture, emitted.source());
            } else {
                assertFalse(emitted.isStub(), fixture.name() + " must render from a body template");
                assertStdoutMatchesOriginal(fixture, emittedSource, fixtureWork);
            }
        }
    }

    private static List<Fixture> loadFixtures() throws IOException {
        Path fixturesPath = repoRoot().resolve("implementations/java/conformance/fixtures");
        assertTrue(Files.isDirectory(fixturesPath), "missing Java conformance fixture directory: " + fixturesPath);
        try (Stream<Path> paths = Files.list(fixturesPath)) {
            List<Path> fixtureJsons = paths
                .filter(Files::isDirectory)
                .map(path -> path.resolve("fixture.json"))
                .filter(Files::isRegularFile)
                .sorted(Comparator.comparing(path -> path.getParent().getFileName().toString()))
                .toList();
            List<Fixture> fixtures = new ArrayList<>();
            for (Path fixtureJson : fixtureJsons) {
                fixtures.add(readFixture(fixtureJson));
            }
            return fixtures;
        }
    }

    private static Fixture readFixture(Path fixtureJson) throws IOException {
        Jcs.Json parsed = Jcs.parse(Files.readString(fixtureJson, StandardCharsets.UTF_8));
        if (!(parsed instanceof Jcs.Obj root)) {
            throw new IllegalArgumentException(fixtureJson + " must be a JSON object");
        }
        Path fixtureDir = fixtureJson.getParent();
        Path originalPath = fixtureDir.resolve(root.stringField("original_source"));
        String originalSource = Files.readString(originalPath, StandardCharsets.UTF_8);
        String behavior = root.stringField("behavior");
        List<List<String>> declaredInputs = stringMatrix(root.arrayField("declared_test_inputs"));
        List<String> expectedOutput = stringList(root.arrayField("expected_output"));
        if (!"carrier_comment".equals(behavior)) {
            assertEquals(
                declaredInputs.size(),
                expectedOutput.size(),
                root.stringField("name") + " input and output cardinality mismatch"
            );
        }
        return new Fixture(
            root.stringField("name"),
            fixtureDir,
            originalPath.getFileName().toString(),
            originalSource,
            root.stringFieldOrNull("original_main_class"),
            behavior,
            declaredInputs,
            expectedOutput,
            readEmission(root.objectField("emission"))
        );
    }

    private static Emission readEmission(Jcs.Obj obj) {
        return new Emission(
            obj.stringField("function"),
            stringList(obj.arrayField("params")),
            stringList(obj.arrayField("param_types")),
            obj.stringField("return_type"),
            obj.stringField("concept_name"),
            readTransportedOperation(obj.get("transported_operation"))
        );
    }

    private static TransportedOperation readTransportedOperation(Jcs.Json json) {
        if (!(json instanceof Jcs.Obj obj)) {
            return null;
        }
        return new TransportedOperation(
            obj.stringField("concept_cid"),
            obj.stringField("concept_site_cid"),
            obj.stringField("loss_record_cid"),
            obj.stringField("operation_kind"),
            obj.stringField("policy_cid"),
            obj.stringField("shape_cid"),
            intList(obj.arrayField("term_position")),
            obj.get("args_jcs"),
            obj.stringFieldOrNull("args_jcs_cid"),
            obj.stringFieldOrNull("sugar_dict_cid"),
            obj.stringFieldOrNull("callsite_cid"),
            obj.stringFieldOrNull("concept_name"),
            obj.stringFieldOrNull("target_library_tag")
        );
    }

    private static void assertStdoutMatchesOriginal(Fixture fixture, Path emittedSource, Path fixtureWork)
            throws Exception {
        Path originalDir = fixtureWork.resolve("original");
        Files.createDirectories(originalDir);
        Path originalSource = originalDir.resolve(fixture.originalSourceFile());
        Files.writeString(originalSource, fixture.originalSource(), StandardCharsets.UTF_8);
        compileJava(originalDir, originalSource.getFileName().toString());

        Path emittedDir = emittedSource.getParent();
        Path harness = emittedDir.resolve("EmittedHarness.java");
        Files.writeString(harness, emittedHarnessSource(fixture.emission()), StandardCharsets.UTF_8);
        compileJava(emittedDir, "EmittedHarness.java", "-cp", ".");

        for (int i = 0; i < fixture.declaredTestInputs().size(); i++) {
            List<String> args = fixture.declaredTestInputs().get(i);
            String expected = fixture.expectedOutput().get(i);
            CommandResult original = runJava(originalDir, fixture.originalMainClass(), args);
            assertBehavior(fixture, expected, original, "original source");

            CommandResult emitted = runJava(emittedDir, "EmittedHarness", args);
            if (!original.stdout().equals(emitted.stdout()) || emitted.exitCode() != 0) {
                fail(CompositionRefusalMemento.of(
                    TARGET_BEHAVIOR_DIVERGENCE,
                    fixture.name() + " emitted output diverged for input " + args
                        + "; original stdout=" + quote(original.stdout())
                        + "; emitted stdout=" + quote(emitted.stdout())
                        + "; emitted exit=" + emitted.exitCode()
                        + "; emitted stderr=" + quote(emitted.stderr())
                ).toJson());
            }
        }
    }

    private static void assertBehavior(
            Fixture fixture,
            String expected,
            CommandResult result,
            String side) {
        if (result.exitCode() != 0 || !expected.equals(result.stdout())) {
            fail(CompositionRefusalMemento.of(
                TARGET_BEHAVIOR_DIVERGENCE,
                fixture.name() + " " + side + " behavior mismatch"
                    + "; expected stdout=" + quote(expected)
                    + "; observed stdout=" + quote(result.stdout())
                    + "; exit=" + result.exitCode()
                    + "; stderr=" + quote(result.stderr())
            ).toJson());
        }
    }

    private static void assertCarrierCommentSurvives(Fixture fixture, String emittedSource) {
        for (String marker : fixture.expectedOutput()) {
            if (!emittedSource.contains(marker)) {
                fail(CompositionRefusalMemento.of(
                    TARGET_BEHAVIOR_DIVERGENCE,
                    fixture.name() + " missing transported-op carrier marker " + quote(marker)
                ).toJson());
            }
        }
        assertTrue(emittedSource.contains("// provekit-concept:"), emittedSource);
        assertTrue(emittedSource.contains("// provekit-concept-payload-cid: blake3-512:"), emittedSource);
    }

    private static Path writeEmittedSource(Fixture fixture, String source, Path emittedDir) throws IOException {
        Files.createDirectories(emittedDir);
        String className = SugarRealizer.snakeToPascal(fixture.emission().function()) + "Transported";
        Path emittedSource = emittedDir.resolve(className + ".java");
        Files.writeString(emittedSource, source, StandardCharsets.UTF_8);
        return emittedSource;
    }

    private static void compileEmittedSource(Path emittedSource) throws IOException, InterruptedException {
        compileJava(emittedSource.getParent(), emittedSource.getFileName().toString());
    }

    private static void compileJava(Path workDir, String sourceFile, String... prefixArgs)
            throws IOException, InterruptedException {
        List<String> command = new ArrayList<>();
        command.add("javac");
        command.addAll(List.of(prefixArgs));
        command.add(sourceFile);
        CommandResult result = run(workDir, command);
        if (result.exitCode() != 0) {
            fail(CompositionRefusalMemento.of(
                TARGET_COMPILE_FAILURE,
                String.join(" ", command) + " failed in " + workDir
                    + "; stdout=" + quote(result.stdout())
                    + "; stderr=" + quote(result.stderr())
            ).toJson());
        }
    }

    private static CommandResult runJava(Path workDir, String mainClass, List<String> args)
            throws IOException, InterruptedException {
        List<String> command = new ArrayList<>();
        command.add("java");
        command.add("-cp");
        command.add(".");
        command.add(mainClass);
        command.addAll(args);
        return run(workDir, command);
    }

    private static CommandResult run(Path workDir, List<String> command) throws IOException, InterruptedException {
        Process process = new ProcessBuilder(command)
            .directory(workDir.toFile())
            .start();
        boolean exited = process.waitFor(Duration.ofSeconds(20).toMillis(), TimeUnit.MILLISECONDS);
        if (!exited) {
            process.destroyForcibly();
            fail(CompositionRefusalMemento.of(
                TARGET_BEHAVIOR_DIVERGENCE,
                String.join(" ", command) + " timed out in " + workDir
            ).toJson());
        }
        return new CommandResult(
            process.exitValue(),
            new String(process.getInputStream().readAllBytes(), StandardCharsets.UTF_8),
            new String(process.getErrorStream().readAllBytes(), StandardCharsets.UTF_8)
        );
    }

    private static String emittedHarnessSource(Emission emission) {
        String className = SugarRealizer.snakeToPascal(emission.function()) + "Transported";
        String mappedReturn = SugarRealizer.mapSourceType(emission.returnType());
        String invocation = className + "." + emission.function() + "(" + invocationArgs(emission) + ")";
        if ("void".equals(mappedReturn)) {
            return """
                final class EmittedHarness {
                    public static void main(String[] args) {
                        %s;
                    }
                }
                """.formatted(invocation);
        }
        return """
            final class EmittedHarness {
                public static void main(String[] args) {
                    System.out.println(%s);
                }
            }
            """.formatted(invocation);
    }

    private static String invocationArgs(Emission emission) {
        List<String> args = new ArrayList<>();
        for (int i = 0; i < emission.params().size(); i++) {
            String mapped = SugarRealizer.mapSourceType(i < emission.paramTypes().size()
                ? emission.paramTypes().get(i)
                : "String");
            args.add(argumentExpression(mapped, i));
        }
        return String.join(", ", args);
    }

    private static String argumentExpression(String mappedType, int index) {
        return switch (mappedType) {
            case "int" -> "Integer.parseInt(args[" + index + "])";
            case "long" -> "Long.parseLong(args[" + index + "])";
            case "short" -> "Short.parseShort(args[" + index + "])";
            case "byte" -> "Byte.parseByte(args[" + index + "])";
            case "double" -> "Double.parseDouble(args[" + index + "])";
            case "float" -> "Float.parseFloat(args[" + index + "])";
            case "boolean" -> "Boolean.parseBoolean(args[" + index + "])";
            case "String" -> "args[" + index + "]";
            default -> "args[" + index + "]";
        };
    }

    private static Path repoRoot() {
        Path current = Path.of("").toAbsolutePath();
        for (Path cursor = current; cursor != null; cursor = cursor.getParent()) {
            if (Files.exists(cursor.resolve("implementations/java/pom.xml"))
                    && Files.exists(cursor.resolve("implementations/rust"))) {
                return cursor;
            }
            if (Files.exists(cursor.resolve("pom.xml"))
                    && "java".equals(cursor.getFileName().toString())
                    && cursor.getParent() != null
                    && cursor.getParent().getParent() != null) {
                Path root = cursor.getParent().getParent();
                if (Files.exists(root.resolve("implementations/rust"))) {
                    return root;
                }
            }
        }
        throw new IllegalStateException("could not locate provekit repo root from " + current);
    }

    private static List<String> stringList(Jcs.Arr arr) {
        List<String> out = new ArrayList<>();
        for (Jcs.Json value : arr.values()) {
            if (!(value instanceof Jcs.Str string)) {
                throw new IllegalArgumentException("expected string array");
            }
            out.add(string.value());
        }
        return out;
    }

    private static List<List<String>> stringMatrix(Jcs.Arr arr) {
        List<List<String>> out = new ArrayList<>();
        for (Jcs.Json value : arr.values()) {
            if (!(value instanceof Jcs.Arr row)) {
                throw new IllegalArgumentException("expected array of string arrays");
            }
            out.add(stringList(row));
        }
        return out;
    }

    private static List<Integer> intList(Jcs.Arr arr) {
        List<Integer> out = new ArrayList<>();
        for (Jcs.Json value : arr.values()) {
            if (!(value instanceof Jcs.Num number)) {
                throw new IllegalArgumentException("expected integer array");
            }
            out.add((int) number.value());
        }
        return out;
    }

    private static String quote(String raw) {
        return raw == null ? "null" : raw.replace("\\", "\\\\").replace("\n", "\\n");
    }

    private record Fixture(
        String name,
        Path directory,
        String originalSourceFile,
        String originalSource,
        String originalMainClass,
        String behavior,
        List<List<String>> declaredTestInputs,
        List<String> expectedOutput,
        Emission emission) {}

    private record Emission(
        String function,
        List<String> params,
        List<String> paramTypes,
        String returnType,
        String conceptName,
        TransportedOperation transportedOperation) {
        SugarRealizer.Realization emit() {
            return SugarRealizer.emitStub(
                function,
                params,
                paramTypes,
                returnType,
                conceptName,
                "",
                List.of(),
                null,
                List.of(),
                transportedOperation
            );
        }
    }

    private record CommandResult(int exitCode, String stdout, String stderr) {}

    private record CompositionRefusalMemento(String failureKind, String failureDetail) {
        static CompositionRefusalMemento of(String failureKind, String failureDetail) {
            return new CompositionRefusalMemento(failureKind, failureDetail);
        }

        String toJson() {
            return "{"
                + "\"kind\":\"CompositionRefusalMemento\","
                + "\"failure_kind\":\"" + jsonEscape(failureKind) + "\","
                + "\"failure_detail\":\"" + jsonEscape(failureDetail) + "\""
                + "}";
        }

        private static String jsonEscape(String raw) {
            return raw
                .replace("\\", "\\\\")
                .replace("\"", "\\\"")
                .replace("\n", "\\n")
                .replace("\r", "\\r");
        }
    }
}
