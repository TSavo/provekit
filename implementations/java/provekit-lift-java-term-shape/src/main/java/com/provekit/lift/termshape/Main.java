package com.provekit.lift.termshape;

import com.github.javaparser.StaticJavaParser;
import com.github.javaparser.ast.CompilationUnit;
import com.github.javaparser.ast.body.MethodDeclaration;
import com.provekit.ir.Jcs;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;

/**
 * CLI entrypoint for body-structural lift of java sources.
 *
 * <p>Usage:
 *   {@code java -jar provekit-lift-java-term-shape.jar <file.java> [--out <ir.json>]}
 *
 * <p>Produces a ProofIR document with one term_shape per method in the
 * input, plus an {@code observed_loss_record} naming every AST node the
 * lifter didn't yet have a recognizer for. The loss_record IS the
 * empirical work list — each entry says exactly which java pattern the
 * substrate emits but this lifter doesn't yet read.
 */
public final class Main {
    public static void main(String[] args) throws IOException {
        if (args.length == 0) {
            System.err.println("usage: provekit-lift-java-term-shape <file.java> [--out <ir.json>]");
            System.exit(2);
        }
        Path inputPath = Path.of(args[0]);
        Path outputPath = null;
        for (int i = 1; i < args.length; i++) {
            if ("--out".equals(args[i]) && i + 1 < args.length) {
                outputPath = Path.of(args[i + 1]);
                i++;
            }
        }
        String source = Files.readString(inputPath);
        CompilationUnit cu = StaticJavaParser.parse(source);

        TermShapeLifter lifter = new TermShapeLifter();
        List<Jcs.Json> entries = new ArrayList<>();
        List<Jcs.Json> losses = new ArrayList<>();

        for (MethodDeclaration method : cu.findAll(MethodDeclaration.class)) {
            // Stop at the @boundary — only lift @sugar bodies.
            // Substrate-honest: boundaries mark "lift gives up; realize
            // fills in." Walking into a boundary primitive's body is
            // off-substrate.
            //
            // Recognition: our java lower prefixes every @sugar method
            // with `// concept: concept:X`. Methods without this header
            // are boundaries (hand-written shim primitives) — emit as
            // boundary references, not lifted bodies.
            String conceptHeader = method.getComment()
                    .map(c -> c.getContent().trim())
                    .filter(c -> c.startsWith("concept:"))
                    .orElse(null);
            if (conceptHeader == null) {
                continue;
            }
            // Self-declaration short-circuit: the method header declares
            // its concept. The IDENTITY of this method's term_shape IS
            // a concept-ref leaf to the declared concept. Both lifter
            // paths (citation-driven and syntax-driven) observe the
            // same header and produce the same leaf — convergence is
            // structural, not coincidental.
            //
            // The body walk still runs to produce realize metadata
            // (param_names, param_types, return_type, structural detail
            // attached as `body_shape`), but the canonical term_shape
            // at this level is the declared concept.
            //
            // If the concept isn't in the live catalogue, the
            // body_shape becomes the structural definition the
            // catalogue accepts on first sight — the concept is
            // lifted into existence.
            TermShapeLifter.LiftedMethod lifted = lifter.liftMethod(method);
            Jcs.Json termShape = Jcs.object(
                "concept_name", Jcs.string(conceptHeader),
                "kind", Jcs.string("concept-ref")
            );
            entries.add(Jcs.object(
                "kind", Jcs.string("lift-term-shape-entry"),
                "function", Jcs.string(method.getNameAsString()),
                "term_shape", termShape,
                "body_shape", lifted.termShape(),
                "param_names", new Jcs.Arr(lifted.paramNames()),
                "param_types", new Jcs.Arr(lifted.paramTypes()),
                "return_type", Jcs.string(lifted.returnType())
            ));
            losses.addAll(lifted.lossRecords());
        }

        Jcs.Obj document = (Jcs.Obj) Jcs.object(
            "kind", Jcs.string("ir-document"),
            "sourceLanguage", Jcs.string("java"),
            "sourcePath", Jcs.string(inputPath.toString()),
            "ir", new Jcs.Arr(entries),
            "observed_loss_record", aggregateLosses(losses)
        );

        String encoded = Jcs.encode(document);
        if (outputPath != null) {
            Files.writeString(outputPath, encoded);
            System.err.println("lift: wrote " + outputPath
                + " (" + entries.size() + " term(s), "
                + losses.size() + " loss entries)");
        } else {
            System.out.println(encoded);
        }
    }

    /** Aggregate loss entries by dimension (node_class) for the sidecar
     *  view. Each dimension lists every occurrence with source location. */
    private static Jcs.Json aggregateLosses(List<Jcs.Json> losses) {
        if (losses.isEmpty()) return Jcs.object();
        java.util.Map<String, List<Jcs.Json>> byDim = new java.util.TreeMap<>();
        for (Jcs.Json entry : losses) {
            if (!(entry instanceof Jcs.Obj obj)) continue;
            String dim = null;
            for (Jcs.Field f : obj.fields()) {
                if ("dimension".equals(f.key()) && f.value() instanceof Jcs.Str s) {
                    dim = s.value();
                }
            }
            if (dim == null) continue;
            byDim.computeIfAbsent(dim, k -> new ArrayList<>()).add(entry);
        }
        List<Jcs.Field> fields = new ArrayList<>();
        for (var e : byDim.entrySet()) {
            fields.add(new Jcs.Field(e.getKey(), new Jcs.Arr(e.getValue())));
        }
        return new Jcs.Obj(fields);
    }
}
