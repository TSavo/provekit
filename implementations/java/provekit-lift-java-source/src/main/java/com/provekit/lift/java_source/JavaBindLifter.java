// SPDX-License-Identifier: Apache-2.0
//
// Java bind-lift kit: walks Java source via the javac compiler API and emits
// `bind-lift-entry` records per `protocol/specs/2026-05-13-bind-ir-lift-result.md`.
//
// ProofIR is concept-language. This kit's job is to walk Java methods and emit
// a LANGUAGE-NEUTRAL term_shape (body/if/while/for/exit/assign/let/rel/bin/call/
// opaque) plus the explicit `// concept: NAME` annotation. cmd_bind clusters
// by term_shape_cid and resolves concepts by `concept_annotation`; neither the
// dispatcher nor downstream verbs receive any Java-surface ops.
//
// Substrate-honest extensions (paper 24 §3 parity with walk_rpc.rs):
//   - @ProveKitSugar.loss() populates loss_record_contribution.value.entries
//   - @ProveKitSugar.observedDimension() sets observed_dimension on the entry
//   - @ProveKitRefuse on TYPE emits a refusal-memento IR record per occurrence
//
// Counterpart: `implementations/rust/provekit-walk/src/bin/walk_rpc.rs::bind_lift`
// (Rust does the same walk over `syn::ItemFn` and emits identical-shape records).

package com.provekit.lift.java_source;

import com.provekit.ir.Jcs;
import com.provekit.lift.JavaAstTemplates;
import com.sun.source.tree.AnnotationTree;
import com.sun.source.tree.AssignmentTree;
import com.sun.source.tree.BinaryTree;
import com.sun.source.tree.BlockTree;
import com.sun.source.tree.ClassTree;
import com.sun.source.tree.CompilationUnitTree;
import com.sun.source.tree.CompoundAssignmentTree;
import com.sun.source.tree.DoWhileLoopTree;
import com.sun.source.tree.EnhancedForLoopTree;
import com.sun.source.tree.ExpressionTree;
import com.sun.source.tree.ExpressionStatementTree;
import com.sun.source.tree.ForLoopTree;
import com.sun.source.tree.IfTree;
import com.sun.source.tree.IdentifierTree;
import com.sun.source.tree.LiteralTree;
import com.sun.source.tree.MemberSelectTree;
import com.sun.source.tree.MethodInvocationTree;
import com.sun.source.tree.MethodTree;
import com.sun.source.tree.NewArrayTree;
import com.sun.source.tree.NewClassTree;
import com.sun.source.tree.ParenthesizedTree;
import com.sun.source.tree.ReturnTree;
import com.sun.source.tree.StatementTree;
import com.sun.source.tree.Tree;
import com.sun.source.tree.TypeCastTree;
import com.sun.source.tree.VariableTree;
import com.sun.source.tree.WhileLoopTree;
import com.sun.source.util.JavacTask;
import com.sun.source.util.TreePath;
import com.sun.source.util.TreePathScanner;
import com.sun.source.util.Trees;
import java.io.IOException;
import java.net.URI;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.HashMap;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Optional;
import javax.lang.model.element.ExecutableElement;
import javax.lang.model.element.Modifier;
import javax.lang.model.element.VariableElement;
import javax.lang.model.type.TypeKind;
import javax.lang.model.type.TypeMirror;
import javax.tools.Diagnostic;
import javax.tools.DiagnosticCollector;
import javax.tools.JavaCompiler;
import javax.tools.SimpleJavaFileObject;
import javax.tools.ToolProvider;

public final class JavaBindLifter {
    private static final String CONCEPT_CITATION_COMMENT_KIND = "provekit-concept-citation-comment-sugar";

    /**
     * Parsed result of a @ProveKitSugar annotation on a method.
     * Carries concept, library, loss dimensions, observed_dimension
     * (for contract-observation bindings), AND the #1357 floating-axis
     * pins family + version (empty string ↔ floating).
     */
    record SugarBinding(
            String concept,
            String library,
            List<String> loss,
            String observedDimension,
            String family,
            String version) {}

    /**
     * Parsed result of a @ProveKitRefuse annotation on a type.
     */
    record RefuseBinding(
            String surface,
            String concept,
            String reason,
            String wouldCloseWithCluster) {}

    /** Walk a workspace and emit one bind-lift-entry per method. */
    public Result liftPaths(String workspaceRoot, List<String> sourcePaths) {
        Path root = Path.of(workspaceRoot).toAbsolutePath().normalize();
        List<Jcs.Json> entries = new ArrayList<>();
        List<Jcs.Json> diagnostics = new ArrayList<>();

        for (String sourcePath : sourcePaths) {
            Path resolved = root.resolve(sourcePath).toAbsolutePath().normalize();
            if (!resolved.equals(root) && !resolved.startsWith(root)) {
                diagnostics.add(diag("error",
                    "path '" + sourcePath + "' escapes workspace root '" + root + "'"));
                continue;
            }
            try {
                if (Files.isDirectory(resolved)) {
                    try (var stream = Files.walk(resolved)) {
                        for (Path javaFile : stream.filter(p -> p.toString().endsWith(".java")).sorted().toList()) {
                            liftFile(root, javaFile, entries, diagnostics);
                        }
                    }
                } else if (Files.exists(resolved) && resolved.toString().endsWith(".java")) {
                    liftFile(root, resolved, entries, diagnostics);
                } else {
                    diagnostics.add(diag("warning", "path not found or not .java: " + resolved));
                }
            } catch (IOException e) {
                diagnostics.add(diag("error", "read failed for " + resolved + ": " + e.getMessage()));
            }
        }
        return new Result(entries, diagnostics);
    }

    Result liftPathsFromSource(String sourcePath, String source) {
        List<Jcs.Json> entries = new ArrayList<>();
        List<Jcs.Json> diagnostics = new ArrayList<>();
        liftSource(sourcePath, source, entries, diagnostics);
        return new Result(entries, diagnostics);
    }

    private void liftFile(Path root, Path javaFile, List<Jcs.Json> entries, List<Jcs.Json> diagnostics) {
        String rel = root.relativize(javaFile).toString().replace('\\', '/');
        String source;
        try {
            source = Files.readString(javaFile, StandardCharsets.UTF_8);
        } catch (IOException e) {
            diagnostics.add(diag("error", "read failed for " + javaFile + ": " + e.getMessage()));
            return;
        }
        liftSource(rel, source, entries, diagnostics);
    }

    private void liftSource(String rel, String source, List<Jcs.Json> entries, List<Jcs.Json> diagnostics) {
        JavaCompiler compiler = ToolProvider.getSystemJavaCompiler();
        if (compiler == null) {
            diagnostics.add(diag("error", "JDK compiler API unavailable; cannot parse " + rel));
            return;
        }
        DiagnosticCollector<javax.tools.JavaFileObject> dc = new DiagnosticCollector<>();
        JavaFileSource fileObj = new JavaFileSource(rel, source);
        List<String> options = List.of("-proc:none", "-Xlint:none");
        JavacTask task = (JavacTask) compiler.getTask(null, null, dc, options, null, List.of(fileObj));
        Iterable<? extends CompilationUnitTree> units;
        try {
            units = task.parse();
            try { task.analyze(); } catch (Throwable ignored) {}
        } catch (Throwable e) {
            diagnostics.add(diag("error", "parse failed for " + rel + ": " + e.getMessage()));
            return;
        }
        for (Diagnostic<?> d : dc.getDiagnostics()) {
            diagnostics.add(diag(d.getKind().name().toLowerCase(Locale.ROOT), d.getMessage(Locale.ROOT)));
        }
        Trees trees = Trees.instance(task);
        for (CompilationUnitTree unit : units) {
            // #41: capture file-level imports. Bodies use SHORT class names
            // (e.g. List<String>) because the source file's `import` lines
            // resolve them. When the body is extracted into a body-template,
            // those imports go away — and the materialized output then has
            // unresolvable short names. Lift the imports here and pass to
            // MethodScanner so each entry can carry the file's imports.
            List<String> fileImports = new ArrayList<>();
            try {
                for (com.sun.source.tree.ImportTree imp : unit.getImports()) {
                    if (imp.isStatic()) continue;
                    String fqn = imp.getQualifiedIdentifier().toString();
                    // Skip wildcards and java.lang.* — wildcards are not
                    // useful in a stripped body; java.lang is implicit.
                    if (fqn.endsWith(".*")) continue;
                    if (fqn.startsWith("java.lang.")) continue;
                    // Skip provekit's lift-time annotation package — the
                    // @ProveKitSugar annotation is a build-time marker;
                    // it's not on the materialized output's runtime classpath.
                    if (fqn.startsWith("com.provekit.lift.")) continue;
                    fileImports.add(fqn);
                }
            } catch (RuntimeException ignored) {
                // Substrate-honest: parse failures already reported above;
                // an empty file_imports just means short-name bodies won't
                // be augmented (downstream gate catches it).
            }
            // #1390: extract top-level class static fields as helpers.
            // Bodies reference these as short names (e.g. `MAPPER.readTree`);
            // the assembler hoists them into the outer compilation unit.
            // Carried as a STRUCTURED field on the binding entry — NOT
            // prepended to body_text, which broke cross-lang matching when
            // attempted that way previously.
            List<String> fileHelpers = new ArrayList<>();
            try {
                for (com.sun.source.tree.Tree typeDecl : unit.getTypeDecls()) {
                    if (!(typeDecl instanceof ClassTree classTree)) continue;
                    for (com.sun.source.tree.Tree member : classTree.getMembers()) {
                        if (!(member instanceof VariableTree var)) continue;
                        if (var.getModifiers() == null) continue;
                        if (!var.getModifiers().getFlags().contains(javax.lang.model.element.Modifier.STATIC)) continue;
                        long start = trees.getSourcePositions().getStartPosition(unit, var);
                        long end = trees.getSourcePositions().getEndPosition(unit, var);
                        if (start < 0 || end <= start) continue;
                        String fieldSrc = source.substring((int) start, (int) end).trim();
                        if (!fieldSrc.endsWith(";")) fieldSrc = fieldSrc + ";";
                        // Strip access modifier so the assembler can hoist
                        // as package-private at the outer class scope.
                        fieldSrc = fieldSrc
                            .replaceFirst("^\\s*private\\s+", "")
                            .replaceFirst("^\\s*public\\s+", "")
                            .replaceFirst("^\\s*protected\\s+", "");
                        fileHelpers.add(fieldSrc.replaceAll("\\s+", " ").trim());
                    }
                }
            } catch (RuntimeException ignored) {
                // Best-effort: empty fileHelpers means MAPPER-style references
                // in bodies won't resolve in materialized output (the compile-
                // check gate surfaces it).
            }
            new MethodScanner(trees, rel, source, entries, diagnostics, fileImports, fileHelpers).scan(unit, null);
            extractRefusals(unit, entries);
        }
    }

    /** Walks methods of a compilation unit and pushes one bind-lift-entry per method. */
    private static final class MethodScanner extends TreePathScanner<Void, Void> {
        private final Trees trees;
        private final String rel;
        private final String source;
        private final List<Jcs.Json> entries;
        private final List<Jcs.Json> diagnostics;
        /// #41: file-level imports lifted from the compilation unit.
        /// Attached to each binding entry's body so materialize can
        /// resolve short-name references in the body.
        private final List<String> fileImports;
        /// #1390: top-level class static fields (helpers). Emitted as a
        /// structured field on each binding entry; the assembler hoists
        /// them into the outer compilation unit, deduplicated.
        private final List<String> fileHelpers;

        MethodScanner(
                Trees trees,
                String rel,
                String source,
                List<Jcs.Json> entries,
                List<Jcs.Json> diagnostics,
                List<String> fileImports,
                List<String> fileHelpers) {
            this.trees = trees;
            this.rel = rel;
            this.source = source;
            this.entries = entries;
            this.diagnostics = diagnostics;
            this.fileImports = fileImports;
            this.fileHelpers = fileHelpers;
        }

        @Override
        public Void visitMethod(MethodTree method, Void unused) {
            TreePath path = getCurrentPath();
            if (method.getBody() == null) return null;
            // Skip constructors.
            if (method.getName().contentEquals("<init>")) return null;

            String fnName = method.getName().toString();
            int line = (int) trees.getSourcePositions().getStartPosition(
                path.getCompilationUnit(), method);
            // Translate offset to 1-based line number.
            int fnLine = lineOf(source, line);

            // Param names + types from the AST (not the resolved element, which
            // is more robust to attribution failures).
            List<Jcs.Json> paramNames = new ArrayList<>();
            List<Jcs.Json> paramTypes = new ArrayList<>();
            for (VariableTree param : method.getParameters()) {
                paramNames.add(Jcs.string(param.getName().toString()));
                paramTypes.add(Jcs.string(param.getType().toString()));
            }
            String returnType;
            if (method.getReturnType() == null) {
                returnType = "()";  // Constructor or void shape; ctors skipped above.
            } else {
                String rt = method.getReturnType().toString();
                returnType = "void".equals(rt) ? "()" : rt;
            }
            // Promote element-resolved types when attribution succeeded.
            javax.lang.model.element.Element el = trees.getElement(path);
            if (el instanceof ExecutableElement ex) {
                paramTypes.clear();
                for (VariableElement p : ex.getParameters()) {
                    paramTypes.add(Jcs.string(typeName(p.asType())));
                }
                TypeMirror ret = ex.getReturnType();
                if (ret != null && ret.getKind() != TypeKind.NONE) {
                    returnType = ret.getKind() == TypeKind.VOID ? "()" : typeName(ret);
                }
            }

            ShapeResult shapeResult = shapeOfStatement(method.getBody());
            Jcs.Json termShape = shapeResult.shape();
            String termShapeCid = Jcs.cid(termShape);

            String conceptAnnotation = extractConceptAnnotation(source, fnLine);
            long bodyStartOffset = trees.getSourcePositions().getStartPosition(
                path.getCompilationUnit(), method.getBody());
            long bodyEndOffset = trees.getSourcePositions().getEndPosition(
                path.getCompilationUnit(), method.getBody());
            int bodyStartLine = bodyStartOffset >= 0 && bodyStartOffset <= Integer.MAX_VALUE
                ? lineOf(source, (int) bodyStartOffset)
                : fnLine;
            int bodyEndLine = bodyEndOffset >= 0 && bodyEndOffset <= Integer.MAX_VALUE
                ? lineOf(source, (int) bodyEndOffset)
                : fnLine;
            List<Jcs.Json> surfaceWitnesses = new ArrayList<>();
            surfaceWitnesses.addAll(observationTagWitnesses(source, bodyStartLine, bodyEndLine));
            surfaceWitnesses.addAll(contractTagWitnesses(source, fnLine, bodyStartLine, bodyEndLine));
            ConceptCitationScan conceptCitationScan = conceptCitationTags(
                source,
                rel,
                fnLine,
                bodyStartLine,
                bodyEndLine,
                diagnostics);
            if (conceptCitationScan.refuseRelift()) {
                return null;
            }

            Jcs.Obj entry = Jcs.object(
                "attr_post", Jcs.nullValue(),
                "attr_pre", Jcs.nullValue(),
                "concept_annotation", conceptAnnotation == null ? Jcs.nullValue() : Jcs.string(conceptAnnotation),
                "concept_citations", Jcs.array(conceptCitationScan.citations()),
                "file", Jcs.string(rel),
                "fn_line", Jcs.integer(fnLine),
                "fn_name", Jcs.string(fnName),
                "kind", Jcs.string("bind-lift-entry"),
                "operand_bindings", Jcs.array(shapeResult.operandBindings()),
                "param_names", Jcs.array(paramNames),
                "param_types", Jcs.array(paramTypes),
                "return_type", Jcs.string(returnType),
                "source_function_name", Jcs.string(fnName),
                "term_shape", termShape,
                "term_shape_cid", Jcs.string(termShapeCid),
                "witnesses", Jcs.array(surfaceWitnesses)
            );
            entries.add(entry);

            Optional<SugarBinding> sugarAnnotation = extractSugarAnnotation(method, trees, path);
            if (sugarAnnotation.isPresent()) {
                SugarBinding binding = sugarAnnotation.get();
                String conceptName = binding.concept();
                String targetLibraryTag = binding.library();
                List<Jcs.Json> lossEntries = binding.loss().stream()
                    .map(Jcs::string)
                    .collect(java.util.stream.Collectors.toList());
                String observedDim = binding.observedDimension();
                Jcs.Obj signatureShape = Jcs.object(
                    "param_names", Jcs.array(paramNames),
                    "param_types", Jcs.array(paramTypes),
                    "return_type", Jcs.string(returnType)
                );
                String signatureShapeCid = Jcs.cid(signatureShape);

                long methodStartOffset = trees.getSourcePositions().getStartPosition(
                    path.getCompilationUnit(), method);
                long methodEndOffset = trees.getSourcePositions().getEndPosition(
                    path.getCompilationUnit(), method);
                int methodStartLine = methodStartOffset >= 0 && methodStartOffset <= Integer.MAX_VALUE
                    ? lineOf(source, (int) methodStartOffset)
                    : fnLine;
                int methodEndLine = methodEndOffset >= 0 && methodEndOffset <= Integer.MAX_VALUE
                    ? lineOf(source, (int) methodEndOffset)
                    : fnLine;
                int methodStartCol = methodStartOffset >= 0 && methodStartOffset <= Integer.MAX_VALUE
                    ? columnOf(source, (int) methodStartOffset)
                    : 0;
                int methodEndCol = methodEndOffset >= 0 && methodEndOffset <= Integer.MAX_VALUE
                    ? columnOf(source, (int) methodEndOffset)
                    : 0;

                String[] srcLines = source.split("\n", -1);
                int startIdx = Math.max(0, methodStartLine - 1);
                int endIdx = Math.min(srcLines.length, methodEndLine);
                StringBuilder spanText = new StringBuilder();
                for (int i = startIdx; i < endIdx; i++) {
                    spanText.append(srcLines[i]).append("\n");
                }
                String sourceCid = Jcs.blake3_512(spanText.toString().getBytes(StandardCharsets.UTF_8));

                // Substrate-honest body capture: the @ProveKitSugar annotation
                // + signature + braces are presentation/sugar. Only the body
                // statements (between the outermost `{` and matching `}`)
                // survive into the substrate. The lifter has already read the
                // annotation to extract concept/library/family/version + read
                // the signature to extract param/return types — those facts
                // live as typed fields on the binding entry (concept_name,
                // target_library_tag, family, library_version, param_names,
                // param_types, return_type). body_text carries only the
                // remaining substance — what the function actually DOES.
                String bodyOnly = extractMethodBodyStatements(spanText.toString());
                JavaAstTemplates.TemplateInfo bodyTemplate =
                    JavaAstTemplates.fromMethodSource(spanText.toString());

                // #41: prepend file-level imports as a comment so the realize
                // plugin's FQN-extracting regex picks them up. The comment is
                // syntactically harmless in the materialized output; the
                // assembler will emit them as real `import X;` statements
                // (deduplicated across fragments). Bodies still reference
                // short class names (List, JsonNode, etc.) — those resolve
                // against the assembled file's import block.
                if (!fileImports.isEmpty()) {
                    StringBuilder importsLine = new StringBuilder("// __fragment_imports__:");
                    for (int i = 0; i < fileImports.size(); i++) {
                        importsLine.append(' ').append(fileImports.get(i));
                        if (i + 1 < fileImports.size()) importsLine.append(',');
                    }
                    bodyOnly = importsLine.toString() + "\n" + bodyOnly;
                }

                Jcs.Obj bodySource = Jcs.object(
                    "ast_template", bodyTemplate.astTemplate(),
                    "body_text", Jcs.string(bodyOnly),
                    "file", Jcs.string(rel),
                    "param_names", Jcs.array(
                        bodyTemplate.paramNames().stream().map(Jcs::string).toList()
                    ),
                    "source_cid", Jcs.string(sourceCid),
                    "span", Jcs.object(
                        "end_col", Jcs.integer(methodEndCol),
                        "end_line", Jcs.integer(methodEndLine),
                        "start_col", Jcs.integer(methodStartCol),
                        "start_line", Jcs.integer(methodStartLine)
                    ),
                    "template_cid", Jcs.string(bodyTemplate.templateCid())
                );

                // #1369 parametric content-addressing: accumulator for composite
                // CID expansions. Populated by javaTypeToConceptHubSortCid calls;
                // emitted on the entry so realize plugins can decompose composite
                // CIDs into (constructor, args) for parameterized morphism dispatch.
                List<ParametricSortExpansion> parametricExpansions = new ArrayList<>();

                // Build the entry; conditionally append observed_dimension.
                List<Object> entryKvs = new ArrayList<>(List.of(
                    "body_source", bodySource,
                    "concept_name", Jcs.string(conceptName),
                    "kind", Jcs.string("library-sugar-binding-entry"),
                    "loss_record_contribution", Jcs.object(
                        "form", Jcs.string("literal"),
                        "value", Jcs.object("entries", Jcs.array(lossEntries))
                    ),
                    "param_names", Jcs.array(paramNames),
                    "param_types", Jcs.array(paramTypes),
                    "param_sort_cids", Jcs.array(
                        paramTypes.stream()
                            .map(t -> Jcs.string(javaTypeToConceptHubSortCid(((Jcs.Str) t).value(), parametricExpansions)))
                            .toList()
                    ),
                    "return_type", Jcs.string(returnType),
                    "return_sort_cid", Jcs.string(javaTypeToConceptHubSortCid(returnType, parametricExpansions)),
                    "signature_shape_cid", Jcs.string(signatureShapeCid),
                    "source_function_name", Jcs.string(fnName),
                    "target_language", Jcs.string("java"),
                    "target_library_tag", Jcs.string(targetLibraryTag),
                    "term_shape", termShape,
                    "term_shape_cid", Jcs.string(termShapeCid)
                ));
                if (observedDim != null && !observedDim.isEmpty()) {
                    entryKvs.add("observed_dimension");
                    entryKvs.add(Jcs.string(observedDim));
                }
                // #1357 / #1355: surface optional family + library_version pins
                // on the binding entry. Absent on the @ProveKitSugar annotation
                // (empty string) → absent in the emitted JSON (NOT empty string —
                // null/missing is the substrate signal for "this axis floats").
                // Parallel to walk_rpc + TS + Python lifters.
                String family = binding.family();
                if (family != null && !family.isEmpty()) {
                    entryKvs.add("family");
                    entryKvs.add(Jcs.string(family));
                }
                String version = binding.version();
                if (version != null && !version.isEmpty()) {
                    entryKvs.add("library_version");
                    entryKvs.add(Jcs.string(version));
                }
                // #1369: emit parametric content-addressing expansions when any
                // parametric type appeared in the signature. Each expansion
                // captures (composite_cid → constructor + arg CIDs) so realize
                // plugins can decompose composite CIDs for parameterized morphism
                // dispatch.
                if (!parametricExpansions.isEmpty()) {
                    List<Jcs.Json> expValues = new ArrayList<>();
                    for (ParametricSortExpansion exp : parametricExpansions) {
                        List<Jcs.Json> argCidsJson = new ArrayList<>();
                        for (String a : exp.argCids()) argCidsJson.add(Jcs.string(a));
                        expValues.add(Jcs.object(
                            "arg_cids", Jcs.array(argCidsJson),
                            "cid", Jcs.string(exp.cid()),
                            "constructor_cid", Jcs.string(exp.constructorCid())
                        ));
                    }
                    entryKvs.add("parametric_sort_expansions");
                    entryKvs.add(Jcs.array(expValues));
                }
                // #1390: file-level static field helpers. Each entry in the
                // file gets the same helpers list (the assembler dedupes
                // across fragments at compilation-unit assembly time).
                if (!fileHelpers.isEmpty()) {
                    List<Jcs.Json> helperValues = new ArrayList<>();
                    for (String h : fileHelpers) helperValues.add(Jcs.string(h));
                    entryKvs.add("file_helpers");
                    entryKvs.add(Jcs.array(helperValues));
                }
                Jcs.Obj sugarEntry = Jcs.object(entryKvs.toArray());
                entries.add(sugarEntry);
            }
            return super.visitMethod(method, unused);
        }
    }

    // ---- Term-shape mapping ------------------------------------------------
    //
    // Maps javac AST nodes to the language-neutral term-shape kinds defined
    // in `2026-05-13-bind-ir-lift-result.md` §2. Identical structurally to
    // `walk_rpc.rs::term_shape_for_fn` and `cmd_bind::TermShape::from_fn` on
    // the Rust side; same shape, same JCS bytes, same shape_cid.

    private static ShapeResult shapeOfStatement(StatementTree stmt) {
        if (stmt == null) return ShapeResult.empty();
        if (stmt instanceof BlockTree b) {
            List<ShapeResult> children = new ArrayList<>();
            for (StatementTree s : b.getStatements()) {
                ShapeResult child = shapeOfStatement(s);
                if (hasOperatorIdentity(child.shape()) || !child.operandBindings().isEmpty()) {
                    children.add(child);
                }
            }
            return operatorShapeResult("concept:seq", children);
        }
        if (stmt instanceof IfTree t) {
            List<ShapeResult> args = new ArrayList<>();
            args.add(shapeOfExpression(t.getCondition()));
            args.add(shapeOfStatement(t.getThenStatement()));
            args.add(t.getElseStatement() == null ? operatorShapeResult("concept:skip", List.of()) : shapeOfStatement(t.getElseStatement()));
            return operatorShapeResult("concept:conditional", args);
        }
        if (stmt instanceof ReturnTree t) {
            if (t.getExpression() == null) return operatorShapeResult("concept:return", List.of());
            return operatorShapeResult("concept:return", List.of(shapeOfExpression(t.getExpression())));
        }
        if (stmt instanceof VariableTree t) {
            ShapeResult target = leafBinding(t.getName().toString());
            ShapeResult init = t.getInitializer() == null ? literalShape(0) : shapeOfExpression(t.getInitializer());
            return operatorShapeResult("concept:assign", List.of(target, init));
        }
        if (stmt instanceof ExpressionStatementTree es) return shapeOfExpression(es.getExpression());
        if (stmt instanceof com.sun.source.tree.BreakTree || stmt instanceof com.sun.source.tree.ContinueTree) {
            return operatorShapeResult("concept:skip", List.of());
        }
        // Structural loop lifts use canonical substrate operator CIDs:
        // concept:while (cond, body), concept:for (init, cond, step, body),
        // and concept:for-each (var, iterable, body).
        if (stmt instanceof WhileLoopTree t) {
            return operatorShapeResult("concept:while",
                List.of(shapeOfExpression(t.getCondition()), shapeOfStatement(t.getStatement())));
        }
        if (stmt instanceof DoWhileLoopTree t) {
            // do-while = seq(body, while(cond, body)) — decompose to existing primitives.
            ShapeResult body = shapeOfStatement(t.getStatement());
            ShapeResult cond = shapeOfExpression(t.getCondition());
            ShapeResult loop = operatorShapeResult("concept:while", List.of(cond, body));
            return operatorShapeResult("concept:seq", List.of(body, loop));
        }
        if (stmt instanceof ForLoopTree t) {
            // Classic C-style for(init; cond; step; body) — 4-arg concept:for.
            // Java allows MULTIPLE init expressions (e.g. `for (i = 0, j = 0; ...)`)
            // and MULTIPLE update expressions (e.g. `i++, j++`). Wrap each list
            // as concept:seq so all side effects survive the lift.
            ShapeResult init;
            if (t.getInitializer().isEmpty()) {
                init = operatorShapeResult("concept:skip", List.of());
            } else if (t.getInitializer().size() == 1) {
                init = shapeOfStatement(t.getInitializer().get(0));
            } else {
                List<ShapeResult> initParts = new ArrayList<>();
                for (StatementTree s : t.getInitializer()) {
                    initParts.add(shapeOfStatement(s));
                }
                init = operatorShapeResult("concept:seq", initParts);
            }
            ShapeResult cond = t.getCondition() == null
                ? literalShape(true)
                : shapeOfExpression(t.getCondition());
            ShapeResult step;
            if (t.getUpdate().isEmpty()) {
                step = operatorShapeResult("concept:skip", List.of());
            } else if (t.getUpdate().size() == 1) {
                step = shapeOfExpression(t.getUpdate().get(0).getExpression());
            } else {
                List<ShapeResult> stepParts = new ArrayList<>();
                for (ExpressionStatementTree es : t.getUpdate()) {
                    stepParts.add(shapeOfExpression(es.getExpression()));
                }
                step = operatorShapeResult("concept:seq", stepParts);
            }
            ShapeResult body = shapeOfStatement(t.getStatement());
            return operatorShapeResult("concept:for", List.of(init, cond, step, body));
        }
        if (stmt instanceof EnhancedForLoopTree t) {
            // Enhanced for (`for (T var : iter) { body }`) — 3-arg concept:for-each.
            ShapeResult var = leafBinding(t.getVariable().getName().toString());
            ShapeResult iterable = shapeOfExpression(t.getExpression());
            ShapeResult body = shapeOfStatement(t.getStatement());
            return operatorShapeResult("concept:for-each", List.of(var, iterable, body));
        }
        // try { body } catch (...) { ... } finally { ... }
        // Structural lift preserving exception type, binding name, AND finally:
        //   concept:try(body, catch-arm1, ..., catch-armN, finally-arm?)
        // Each catch-arm = concept:catch-arm(exception-type-symbol, binding-name-symbol, body).
        // Finally-arm = concept:finally-arm(body) — only emitted when present so
        // the realize side can detect it by checking the trailing-arg's operator.
        if (stmt instanceof com.sun.source.tree.TryTree t) {
            List<ShapeResult> args = new ArrayList<>();
            args.add(shapeOfStatement(t.getBlock()));
            for (com.sun.source.tree.CatchTree c : t.getCatches()) {
                String typeText = c.getParameter().getType().toString();
                String bindingName = c.getParameter().getName().toString();
                ShapeResult typeLeaf = new ShapeResult(
                    Jcs.object("kind", Jcs.string("symbol"), "text", Jcs.string(typeText)),
                    List.of());
                ShapeResult bindingLeaf = new ShapeResult(
                    Jcs.object("kind", Jcs.string("symbol"), "text", Jcs.string(bindingName)),
                    List.of());
                ShapeResult catchBody = shapeOfStatement(c.getBlock());
                args.add(operatorShapeResult("concept:catch-arm",
                    List.of(typeLeaf, bindingLeaf, catchBody)));
            }
            if (t.getFinallyBlock() != null) {
                ShapeResult finallyBody = shapeOfStatement(t.getFinallyBlock());
                args.add(operatorShapeResult("concept:finally-arm", List.of(finallyBody)));
            }
            return operatorShapeResult("concept:try", args);
        }
        if (stmt instanceof com.sun.source.tree.ThrowTree t) {
            return operatorShapeResult("concept:throw",
                List.of(shapeOfExpression(t.getExpression())));
        }
        return ShapeResult.empty();
    }

    private static ShapeResult shapeOfExpression(Tree expr) {
        if (expr == null) return ShapeResult.empty();
        if (expr instanceof ParenthesizedTree t) return shapeOfExpression(t.getExpression());
        if (expr instanceof TypeCastTree t) return shapeOfExpression(t.getExpression());
        if (expr instanceof AssignmentTree t) {
            return operatorShapeResult("concept:assign", List.of(ShapeResult.empty(), shapeOfExpression(t.getExpression())));
        }
        if (expr instanceof CompoundAssignmentTree t) {
            return operatorShapeResult("concept:assign", List.of(ShapeResult.empty(), shapeOfExpression(t.getExpression())));
        }
        if (expr instanceof BinaryTree b) {
            String concept = switch (b.getKind()) {
                case PLUS -> "concept:add";
                case MINUS -> "concept:sub";
                case MULTIPLY -> "concept:mul";
                case DIVIDE -> "concept:div";
                case REMAINDER -> "concept:mod";
                case EQUAL_TO -> "concept:eq";
                case NOT_EQUAL_TO -> "concept:ne";
                case LESS_THAN -> "concept:lt";
                case LESS_THAN_EQUAL -> "concept:le";
                case GREATER_THAN -> "concept:gt";
                case GREATER_THAN_EQUAL -> "concept:ge";
                default -> "";
            };
            if (!concept.isEmpty()) {
                return operatorShapeResult(concept, List.of(
                    shapeOfExpression(b.getLeftOperand()),
                    shapeOfExpression(b.getRightOperand())
                ));
            }
        }
        if (expr instanceof IdentifierTree t) return leafBinding(t.getName().toString());
        if (expr instanceof LiteralTree t) return literalShape(t.getValue());
        // Substrate-canonical concept:call shape — MATCHES the rust shape
        // emitted by walk_rpc (Expr::MethodCall):
        //   instance method (receiver.method(args)):
        //     args[0] = receiver shape
        //     args[1] = kind:"method" leaf with method ident
        //     args[2..] = argument shapes
        //   free function call (identifier(args)) — no receiver:
        //     args[0] = kind:"path" leaf with function ident
        //     args[1..] = argument shapes
        // Previously this folded `obj.method` into a single method leaf
        // text="obj.method", which dropped the receiver operand binding
        // and broke cycle invariance with rust's call shape.
        if (expr instanceof MethodInvocationTree mi) {
            List<ShapeResult> args = new ArrayList<>();
            Tree select = mi.getMethodSelect();
            if (select instanceof MemberSelectTree ms) {
                // Instance method: receiver as args[0], method-ident as args[1].
                args.add(shapeOfExpression(ms.getExpression()));
                args.add(new ShapeResult(
                    Jcs.object("kind", Jcs.string("method"),
                               "text", Jcs.string(ms.getIdentifier().toString())),
                    List.of()));
            } else if (select instanceof IdentifierTree id) {
                // Bare identifier call — free function form.
                args.add(pathLeaf(id.getName().toString()));
            } else {
                args.add(calleeLeaf(select));
            }
            for (Tree arg : mi.getArguments()) {
                args.add(shapeOfExpression(arg));
            }
            return operatorShapeResult("concept:call", args);
        }
        if (expr instanceof NewClassTree nc) {
            // Constructor call: callee is the type name as a path leaf.
            List<ShapeResult> args = new ArrayList<>();
            args.add(pathLeaf(nc.getIdentifier().toString()));
            for (Tree arg : nc.getArguments()) {
                args.add(shapeOfExpression(arg));
            }
            return operatorShapeResult("concept:call", args);
        }
        if (expr instanceof MemberSelectTree ms) {
            // Bare member access (e.g. `obj.field`) — lift as a method/path
            // identity leaf. Used when the member access isn't inside a call.
            return calleeLeaf(ms);
        }
        if (expr instanceof NewArrayTree na) {
            // Array constructor: concept:call with array-of as callee + initializers.
            List<ShapeResult> args = new ArrayList<>();
            args.add(pathLeaf("Array"));
            if (na.getInitializers() != null) {
                for (Tree init : na.getInitializers()) {
                    args.add(shapeOfExpression(init));
                }
            }
            return operatorShapeResult("concept:call", args);
        }
        return ShapeResult.empty();
    }

    /**
     * Build a callee identity leaf for a method-select or path expression.
     * Mirrors walk_rpc's rust emission: kind="method" for member-select
     * (receiver.method), kind="path" for bare identifier (free function).
     * The text is a flattened dotted form preserving the source's name chain.
     */
    private static ShapeResult calleeLeaf(Tree calleeExpr) {
        if (calleeExpr instanceof IdentifierTree t) {
            return pathLeaf(t.getName().toString());
        }
        if (calleeExpr instanceof MemberSelectTree ms) {
            String chain = flattenMemberSelect(ms);
            return new ShapeResult(
                Jcs.object("kind", Jcs.string("method"), "text", Jcs.string(chain)),
                List.of()
            );
        }
        return ShapeResult.empty();
    }

    private static ShapeResult pathLeaf(String text) {
        return new ShapeResult(
            Jcs.object("kind", Jcs.string("path"), "text", Jcs.string(text)),
            List.of()
        );
    }

    private static String flattenMemberSelect(MemberSelectTree ms) {
        // Walk a chain like A.B.C.method → "A.B.C.method"
        StringBuilder sb = new StringBuilder();
        flattenInto(ms, sb);
        return sb.toString();
    }

    private static void flattenInto(Tree node, StringBuilder sb) {
        if (node instanceof MemberSelectTree ms) {
            flattenInto(ms.getExpression(), sb);
            if (sb.length() > 0) sb.append('.');
            sb.append(ms.getIdentifier());
        } else if (node instanceof IdentifierTree t) {
            sb.append(t.getName());
        } else {
            sb.append(node.toString());
        }
    }

    private static ShapeResult operatorShapeResult(String conceptName, List<ShapeResult> args) {
        String opCid = conceptCidForName(conceptName);
        if (opCid == null || opCid.isBlank()) return ShapeResult.empty();
        List<Jcs.Json> argShapes = args.stream().map(ShapeResult::shape).toList();
        List<Jcs.Json> bindings = new ArrayList<>();
        for (int i = 0; i < args.size(); i++) {
            for (Jcs.Json binding : args.get(i).operandBindings()) {
                bindings.add(prefixBinding(binding, i));
            }
        }
        return new ShapeResult(
            Jcs.object(
                "args", Jcs.array(argShapes),
                "concept_name", Jcs.string(conceptName),
                "op_cid", Jcs.string(opCid)
            ),
            bindings
        );
    }

    private static ShapeResult leafBinding(String symbol) {
        return new ShapeResult(
            Jcs.object(),
            List.of(Jcs.object(
                "position", Jcs.array(),
                "symbol", Jcs.string(symbol)
            ))
        );
    }

    private static ShapeResult literalShape(Object value) {
        Jcs.Json literal;
        if (value == null) {
            literal = Jcs.object("kind", Jcs.string("literal"), "value", Jcs.nullValue());
        } else if (value instanceof Boolean b) {
            literal = Jcs.object("kind", Jcs.string("literal"), "value", Jcs.bool(b));
        } else if (value instanceof Double d) {
            // Bit-preserving: emit {"__float_bits__": <u64>} (IEEE 754 raw bits, substrate #1262).
            // Double.doubleToRawLongBits preserves all bit patterns including NaN, +/-0, infinity.
            long bits = Double.doubleToRawLongBits(d);
            literal = Jcs.object("kind", Jcs.string("literal"), "value",
                Jcs.object("__float_bits__", Jcs.integer(bits)));
        } else if (value instanceof Float f) {
            // Widen to double then preserve all 32 bits via doubleToRawLongBits of the widened form.
            // Java float literals widen to double in the AST; raw float bits via Float.floatToRawIntBits.
            long bits = Float.floatToRawIntBits(f) & 0xFFFFFFFFL;
            literal = Jcs.object("kind", Jcs.string("literal"), "value",
                Jcs.object("__float_bits__", Jcs.integer(bits)));
        } else if (value instanceof Number n) {
            literal = Jcs.object("kind", Jcs.string("literal"), "value", Jcs.integer(n.longValue()));
        } else if (value instanceof String s) {
            literal = Jcs.object("kind", Jcs.string("literal"), "value", Jcs.string(s));
        } else {
            literal = Jcs.object();
        }
        return new ShapeResult(literal, List.of());
    }

    private static Jcs.Json prefixBinding(Jcs.Json binding, int prefix) {
        if (!(binding instanceof Jcs.Obj obj)) return binding;
        Jcs.Json rawPosition = obj.get("position");
        String symbol = obj.stringFieldOrNull("symbol");
        if (!(rawPosition instanceof Jcs.Arr position) || symbol == null) return binding;
        List<Jcs.Json> prefixed = new ArrayList<>();
        prefixed.add(Jcs.integer(prefix));
        prefixed.addAll(position.values());
        return Jcs.object(
            "position", Jcs.array(prefixed),
            "symbol", Jcs.string(symbol)
        );
    }

    private static boolean hasOperatorIdentity(Jcs.Json value) {
        if (value instanceof Jcs.Obj obj) {
            return obj.stringFieldOrNull("concept_name") != null || obj.stringFieldOrNull("op_cid") != null;
        }
        return false;
    }

    // ---- Concept annotation extraction -------------------------------------
    //
    // Pulls the NAME from a `// concept: NAME` line immediately preceding the
    // method declaration. Mirrors the Rust extractor in walk_rpc.rs:
    // alphabetic-prefixed annotations are stripped to their NAME-only form.

    private static String extractConceptAnnotation(String source, int fnLine) {
        if (fnLine <= 1) return null;
        String[] lines = source.split("\n", -1);
        int idx = fnLine - 2; // line above the method declaration line
        while (idx >= 0) {
            String line = lines[idx].stripLeading();
            if (line.startsWith("// concept:")) {
                String rest = line.substring("// concept:".length()).trim();
                if (rest.startsWith("UNNAMED-CONCEPT-")) return null;
                return rest;
            }
            // Skip past sibling bind-emitted annotations and javadocs.
            if (line.startsWith("//") || line.startsWith("@") || line.startsWith("/*") || line.startsWith("*")
                || line.startsWith("*/") || line.isEmpty()) {
                idx--;
                continue;
            }
            break;
        }
        return null;
    }

    private static Optional<SugarBinding> extractSugarAnnotation(MethodTree method, Trees trees, TreePath path) {
        for (AnnotationTree ann : method.getModifiers().getAnnotations()) {
            String annName = ann.getAnnotationType().toString();
            if (!annName.equals("ProveKitSugar") && !annName.endsWith(".ProveKitSugar")) {
                continue;
            }
            String concept = null;
            String library = null;
            List<String> loss = new ArrayList<>();
            String observedDimension = "";
            String family = "";
            String version = "";
            for (ExpressionTree arg : ann.getArguments()) {
                if (!(arg instanceof AssignmentTree assign)) continue;
                String key = assign.getVariable().toString();
                ExpressionTree valExpr = assign.getExpression();
                if ("concept".equals(key)) {
                    concept = unquote(valExpr.toString());
                } else if ("library".equals(key)) {
                    library = unquote(valExpr.toString());
                } else if ("loss".equals(key)) {
                    loss = extractStringArray(valExpr);
                } else if ("observedDimension".equals(key)) {
                    observedDimension = unquote(valExpr.toString());
                } else if ("family".equals(key)) {
                    // #1357 / #1355: optional concept family pin.
                    family = unquote(valExpr.toString());
                } else if ("version".equals(key)) {
                    // #1357 / #1355: optional library version pin.
                    version = unquote(valExpr.toString());
                }
            }
            if (concept != null && !concept.isEmpty() && library != null && !library.isEmpty()) {
                return Optional.of(new SugarBinding(concept, library, loss, observedDimension, family, version));
            }
            return Optional.empty();
        }
        return Optional.empty();
    }

    /**
     * Substrate-honest body extraction: given the full text of a method
     * (annotations + signature + body), return ONLY the statements
     * between the outermost matching braces. Strips leading/trailing
     * whitespace per line, preserving internal indentation relative to
     * a baseline of one indent level (so the substrate captures the
     * body shape without depending on the shim's outer class nesting).
     *
     * Mirrors the rust lifter's behavior — body_text in the rust shim's
     * .proof envelope contains only body statements, not the @sugar
     * attribute or signature. Java parity required so cross-language
     * materialize gets consistent body templates.
     */
    private static String extractMethodBodyStatements(String methodText) {
        int openBrace = -1;
        int depth = 0;
        boolean inString = false;
        boolean inChar = false;
        boolean inLineComment = false;
        boolean inBlockComment = false;
        // Walk the text to find the FIRST unescaped `{` outside of strings/
        // comments; that's the method body's opening brace (any earlier `{`
        // would belong to annotation values, which use `(` not `{` for args
        // but could in principle be in array-valued annotation params like
        // `loss = {...}`). Track depth to find the matching close.
        int closeBrace = -1;
        for (int i = 0; i < methodText.length(); i++) {
            char c = methodText.charAt(i);
            char next = i + 1 < methodText.length() ? methodText.charAt(i + 1) : '\0';
            if (inLineComment) {
                if (c == '\n') inLineComment = false;
                continue;
            }
            if (inBlockComment) {
                if (c == '*' && next == '/') { inBlockComment = false; i++; }
                continue;
            }
            if (inString) {
                if (c == '\\') { i++; continue; }
                if (c == '"') inString = false;
                continue;
            }
            if (inChar) {
                if (c == '\\') { i++; continue; }
                if (c == '\'') inChar = false;
                continue;
            }
            if (c == '/' && next == '/') { inLineComment = true; i++; continue; }
            if (c == '/' && next == '*') { inBlockComment = true; i++; continue; }
            if (c == '"') { inString = true; continue; }
            if (c == '\'') { inChar = true; continue; }
            if (c == '{') {
                // Skip annotation-argument array literals like `loss = {...}`.
                // Heuristic: if we're not yet past the signature (no `)` seen
                // after the LAST opening `(`), the `{` belongs to an annotation.
                // We can detect "past the signature" by looking back for the
                // most recent `)` not inside parens — easier: the method body
                // brace is the one whose `}` is at the end of the method text.
                if (openBrace < 0) {
                    // Look ahead for the matching close at depth 0.
                    int lookDepth = 1;
                    int j = i + 1;
                    boolean ls = false, lc = false, lline = false, lblock = false;
                    while (j < methodText.length() && lookDepth > 0) {
                        char cc = methodText.charAt(j);
                        char nn = j + 1 < methodText.length() ? methodText.charAt(j + 1) : '\0';
                        if (lline) { if (cc == '\n') lline = false; j++; continue; }
                        if (lblock) { if (cc == '*' && nn == '/') { lblock = false; j++; } j++; continue; }
                        if (ls) { if (cc == '\\') j++; else if (cc == '"') ls = false; j++; continue; }
                        if (lc) { if (cc == '\\') j++; else if (cc == '\'') lc = false; j++; continue; }
                        if (cc == '/' && nn == '/') { lline = true; j += 2; continue; }
                        if (cc == '/' && nn == '*') { lblock = true; j += 2; continue; }
                        if (cc == '"') { ls = true; j++; continue; }
                        if (cc == '\'') { lc = true; j++; continue; }
                        if (cc == '{') lookDepth++;
                        else if (cc == '}') lookDepth--;
                        j++;
                    }
                    // If after the matching close there's only whitespace/closing
                    // characters before EOF, this is the method body brace.
                    // Otherwise it's an inner brace (annotation array, etc.).
                    int afterClose = j;
                    boolean isMethodBrace = true;
                    while (afterClose < methodText.length()) {
                        char cc = methodText.charAt(afterClose);
                        if (!Character.isWhitespace(cc)) {
                            isMethodBrace = false;
                            break;
                        }
                        afterClose++;
                    }
                    if (isMethodBrace) {
                        openBrace = i;
                        closeBrace = j - 1;
                        break;
                    }
                }
            }
        }
        if (openBrace < 0 || closeBrace < 0 || closeBrace <= openBrace + 1) {
            // Fallback: couldn't find body braces; return original (callers
            // should be tolerant — body-template fallback applies).
            return methodText;
        }
        String body = methodText.substring(openBrace + 1, closeBrace);
        // Trim leading newline if present; preserve internal indentation.
        if (body.startsWith("\n")) body = body.substring(1);
        if (body.endsWith("\n")) body = body.substring(0, body.length() - 1);
        return body;
    }

    /**
     * Substrate-honest java-syntax → concept-hub sort CID translation.
     *
     * This is the JAVA KIT's internal knowledge of how its source syntax maps
     * to substrate-canonical concept-hub identities. Parallel to what
     * source_transform.rs::rust_source_type_to_concept_hub_sort_cid does for
     * rust at the rust kit/substrate boundary.
     *
     * Concept-hub sort CIDs verified against
     * menagerie/concept-shapes/catalog/sorts/. Returns empty string for
     * unrecognized types (substrate-honest gap signal — the kit doesn't
     * yet know how to lift this java type).
     *
     * NOTE: this lives in the kit (not in cmd_materialize) per the
     * invariant: kit-internal labels never cross to substrate; only
     * concept-hub CIDs do. The translation happens AT the kit boundary.
     */
    /**
     * Carrier-side parametric sort expansion. Same shape as rust's
     * libprovekit::core::lower_plugin::ParametricSortExpansion. Used to
     * communicate (composite_cid → constructor + args) so realize plugins
     * can decompose for parameterized morphism dispatch.
     */
    record ParametricSortExpansion(String cid, String constructorCid, List<String> argCids) {
        /** Compute the composite CID via blake3-512 of JCS-canonicalized form. */
        static String composeCid(String constructorCid, List<String> argCids) {
            List<Jcs.Json> argList = new ArrayList<>();
            for (String a : argCids) argList.add(Jcs.string(a));
            Jcs.Json canonical = Jcs.object(
                "arg_cids", Jcs.array(argList),
                "constructor_cid", Jcs.string(constructorCid),
                "kind", Jcs.string("parametric-sort-application")
            );
            return Jcs.cid(canonical);
        }

        static ParametricSortExpansion build(String constructorCid, List<String> argCids) {
            return new ParametricSortExpansion(composeCid(constructorCid, argCids), constructorCid, argCids);
        }
    }

    /**
     * Catalog-driven kit-source alias lookup (#1370). The Java kit's
     * KitSourceAliasMemento files in menagerie/concept-shapes/catalog/
     * kit-source-aliases/java-*.json declare the source-text tokens
     * that denote each kit-sort. This map is built once on first lift.
     *
     * Three forms:
     * - PRIMITIVE: token → concept-hub sort CID (no parametric args)
     * - PARAMETRIC_CONSTRUCTOR: token denotes a constructor; takes args
     *   at use-site
     * - SHORTHAND: token denotes a fixed parametric application
     *   (e.g. StringBuilder = Ref<String>)
     */
    private record AliasEntry(
        String kind,               // "primitive" / "constructor" / "shorthand"
        String targetCid,          // primitive: concept-hub CID; constructor: constructor CID; shorthand: composite CID
        Integer arity,             // for constructor: expected arity; null otherwise
        String constructorCid,     // for shorthand: the parametric constructor's CID; null otherwise
        List<String> argCids       // for shorthand: pre-bound arg CIDs; null otherwise
    ) {}

    private static volatile Map<String, AliasEntry> ALIAS_MAP = null;

    private static Map<String, AliasEntry> aliasMap() {
        Map<String, AliasEntry> cached = ALIAS_MAP;
        if (cached != null) return cached;
        synchronized (JavaBindLifter.class) {
            if (ALIAS_MAP != null) return ALIAS_MAP;
            ALIAS_MAP = buildAliasMap();
            return ALIAS_MAP;
        }
    }

    private static Map<String, AliasEntry> buildAliasMap() {
        Map<String, AliasEntry> map = new HashMap<>();
        // Walk up from CWD to find menagerie/.
        java.nio.file.Path cwd = java.nio.file.Paths.get(System.getProperty("user.dir", "."));
        java.nio.file.Path root = null;
        for (java.nio.file.Path p = cwd; p != null; p = p.getParent()) {
            if (java.nio.file.Files.isDirectory(p.resolve("menagerie"))) {
                root = p;
                break;
            }
        }
        if (root == null) return map;
        java.nio.file.Path aliasesDir = root.resolve("menagerie")
                .resolve("concept-shapes").resolve("catalog").resolve("kit-source-aliases");
        if (!java.nio.file.Files.isDirectory(aliasesDir)) return map;
        try (java.util.stream.Stream<java.nio.file.Path> files = java.nio.file.Files.list(aliasesDir)) {
            for (java.nio.file.Path file : (Iterable<java.nio.file.Path>) files::iterator) {
                String name = file.getFileName().toString();
                if (!name.startsWith("java-") || !name.endsWith(".json")) continue;
                try {
                    String raw = java.nio.file.Files.readString(file, StandardCharsets.UTF_8);
                    Jcs.Json doc = Jcs.parse(raw);
                    if (!(doc instanceof Jcs.Obj envelope)) continue;
                    Jcs.Json mementoJson = envelope.get("memento");
                    if (!(mementoJson instanceof Jcs.Obj memento)) continue;
                    // Resolve target_cid from sort_morphism_cid via the catalog.
                    String sortMorphismCid = memento.stringFieldOrNull("sort_morphism_cid");
                    String targetCid = resolveMorphismTargetCid(root, sortMorphismCid);
                    if (targetCid == null) continue;
                    Jcs.Json aliasesJ = memento.get("source_aliases");
                    if (!(aliasesJ instanceof Jcs.Arr aliasesArr)) continue;
                    Jcs.Json shorthand = memento.get("denotes_parametric_application");
                    Jcs.Json arityJ = memento.get("parametric_arity");
                    for (Jcs.Json a : aliasesArr.values()) {
                        if (!(a instanceof Jcs.Str s)) continue;
                        String token = s.value();
                        AliasEntry entry;
                        if (shorthand instanceof Jcs.Obj sh) {
                            // Fixed parametric application — bake the composite CID now.
                            String ctorCid = sh.stringFieldOrNull("constructor_cid");
                            Jcs.Json argsJ = sh.get("arg_cids");
                            List<String> argCids = new ArrayList<>();
                            if (argsJ instanceof Jcs.Arr ar) {
                                for (Jcs.Json aa : ar.values()) {
                                    if (aa instanceof Jcs.Str ss) argCids.add(ss.value());
                                }
                            }
                            String compositeCid = ParametricSortExpansion.composeCid(ctorCid, argCids);
                            entry = new AliasEntry("shorthand", compositeCid, null, ctorCid, argCids);
                        } else if (arityJ instanceof Jcs.Num arityN) {
                            // Parametric constructor — target is the constructor CID.
                            entry = new AliasEntry("constructor", targetCid, (int) arityN.value(), null, null);
                        } else {
                            entry = new AliasEntry("primitive", targetCid, null, null, null);
                        }
                        map.putIfAbsent(token, entry);
                    }
                } catch (IOException | IllegalArgumentException ignored) {
                    // Skip malformed; other files still load.
                }
            }
        } catch (IOException ignored) {
            // Empty map; lifter will treat all types as gaps.
        }
        return map;
    }

    /** Read a sort-morphism file by CID; return its target_sort_cid (concept-hub side). */
    private static String resolveMorphismTargetCid(java.nio.file.Path root, String morphismCid) {
        if (morphismCid == null) return null;
        java.nio.file.Path algorithms = root.resolve("menagerie")
                .resolve("concept-shapes").resolve("catalog").resolve("algorithms");
        if (!java.nio.file.Files.isDirectory(algorithms)) return null;
        try (java.util.stream.Stream<java.nio.file.Path> files = java.nio.file.Files.list(algorithms)) {
            for (java.nio.file.Path file : (Iterable<java.nio.file.Path>) files::iterator) {
                String name = file.getFileName().toString();
                if (!name.contains(morphismCid)) continue;
                if (!name.endsWith(".json")) continue;
                String raw = java.nio.file.Files.readString(file, StandardCharsets.UTF_8);
                Jcs.Json doc = Jcs.parse(raw);
                if (!(doc instanceof Jcs.Obj o)) continue;
                Jcs.Json header = o.get("header");
                if (!(header instanceof Jcs.Obj h)) continue;
                String target = h.stringFieldOrNull("target_sort_cid");
                if (target != null) return target;
            }
        } catch (IOException | IllegalArgumentException ignored) {
            return null;
        }
        return null;
    }

    /**
     * #1369 + #1370: catalog-driven java-type → concept-hub sort CID.
     *
     * Looks up source tokens in the kit's signed KitSourceAlias mementos
     * (NO hardcoded names). Parametric types recursively resolve inner
     * arg CIDs and compose composite CIDs via blake3-512(JCS(...)).
     */
    private static String javaTypeToConceptHubSortCid(String javaType, List<ParametricSortExpansion> expansions) {
        if (javaType == null) return "";
        String t = javaType.trim();
        // Parse: "Foo<Bar<Baz>, Qux>" → outer="Foo", args=["Bar<Baz>", "Qux"]
        // "byte[]" → outer="byte[]"; "List<String>" → outer="List", args=["String"]
        String outer = t;
        List<String> argSrcs = new ArrayList<>();
        int generic = t.indexOf('<');
        if (generic > 0 && t.endsWith(">")) {
            outer = t.substring(0, generic).trim();
            String inside = t.substring(generic + 1, t.length() - 1);
            argSrcs = splitTopLevelCommas(inside);
        }
        Map<String, AliasEntry> aliases = aliasMap();
        AliasEntry direct = aliases.get(outer);
        // FQN fallback: javac's TypeMirror.toString() returns FQNs for declared
        // types (e.g. "java.lang.String", "com.fasterxml.jackson.databind.JsonNode").
        // If the alias map doesn't have an entry for the FQN form, try the
        // simple class name. Kit-source-alias mementos that list FQN
        // variants (e.g. "java.util.List", "java.fasterxml.jackson...JsonNode")
        // get a direct hit; primitive/lang-package types fall through to
        // simple-name lookup.
        if (direct == null) {
            int lastDot = outer.lastIndexOf('.');
            if (lastDot >= 0) {
                String simpleName = outer.substring(lastDot + 1);
                direct = aliases.get(simpleName);
            }
        }
        // Handle T[] arrays by recursing on the element type as a List<T>.
        if (direct == null && outer.endsWith("[]")) {
            String elemSrc = outer.substring(0, outer.length() - 2).trim();
            // Check byte[] shorthand first via direct alias lookup
            AliasEntry bytesEntry = aliases.get("byte[]");
            if ((elemSrc.equals("byte") || elemSrc.equals("Byte")) && bytesEntry != null) {
                return bytesEntry.targetCid();
            }
            String innerCid = javaTypeToConceptHubSortCid(elemSrc, expansions);
            if (innerCid.isEmpty()) return "";
            AliasEntry listEntry = aliases.get("List");
            if (listEntry == null || !"constructor".equals(listEntry.kind())) return "";
            ParametricSortExpansion exp = ParametricSortExpansion.build(
                listEntry.targetCid(), List.of(innerCid));
            addExpansionIfMissing(expansions, exp);
            return exp.cid();
        }
        if (direct == null) return "";
        return switch (direct.kind()) {
            case "primitive" -> direct.targetCid();
            case "shorthand" -> {
                // Pre-baked composite CID; emit the expansion so realize can
                // decompose the composite into (constructor, arg_cids).
                ParametricSortExpansion exp = new ParametricSortExpansion(
                    direct.targetCid(),
                    direct.constructorCid(),
                    direct.argCids() == null ? List.of() : direct.argCids());
                addExpansionIfMissing(expansions, exp);
                yield direct.targetCid();
            }
            case "constructor" -> {
                int expectedArity = direct.arity() == null ? 1 : direct.arity();
                if (argSrcs.size() != expectedArity) yield "";
                List<String> argCids = new ArrayList<>();
                for (String a : argSrcs) {
                    String c = javaTypeToConceptHubSortCid(a, expansions);
                    if (c.isEmpty()) yield "";
                    argCids.add(c);
                }
                ParametricSortExpansion exp = ParametricSortExpansion.build(
                    direct.targetCid(), argCids);
                addExpansionIfMissing(expansions, exp);
                yield exp.cid();
            }
            default -> "";
        };
    }

    private static void addExpansionIfMissing(List<ParametricSortExpansion> expansions, ParametricSortExpansion exp) {
        if (expansions.stream().noneMatch(e -> e.cid().equals(exp.cid()))) {
            expansions.add(exp);
        }
    }

    /** Split a top-level comma-separated string respecting nested angle brackets.
     *  "String, List<Integer>" → ["String", "List<Integer>"]. */
    private static List<String> splitTopLevelCommas(String inside) {
        List<String> out = new ArrayList<>();
        int depth = 0;
        StringBuilder cur = new StringBuilder();
        for (int i = 0; i < inside.length(); i++) {
            char c = inside.charAt(i);
            if (c == '<') depth++;
            else if (c == '>') depth--;
            if (c == ',' && depth == 0) {
                out.add(cur.toString().trim());
                cur.setLength(0);
            } else {
                cur.append(c);
            }
        }
        if (cur.length() > 0) out.add(cur.toString().trim());
        return out;
    }

    /** Legacy entry point for callers that don't accumulate expansions yet.
     *  Discards expansions — only safe when caller knows the type is primitive. */
    private static String javaTypeToConceptHubSortCid(String javaType) {
        return javaTypeToConceptHubSortCid(javaType, new ArrayList<>());
    }

    /** Strip surrounding double-quotes from a string literal token, if present. */
    private static String unquote(String s) {
        if (s.startsWith("\"") && s.endsWith("\"") && s.length() >= 2) {
            return s.substring(1, s.length() - 1);
        }
        return s;
    }

    /**
     * Extract a string array from an annotation argument expression.
     * Handles: single string literal, array initializer {@code {"a","b"}},
     * and NewArrayTree from the compiler.
     */
    private static List<String> extractStringArray(ExpressionTree expr) {
        List<String> result = new ArrayList<>();
        if (expr instanceof NewArrayTree arr) {
            if (arr.getInitializers() != null) {
                for (ExpressionTree elem : arr.getInitializers()) {
                    result.add(unquote(elem.toString()));
                }
            }
        } else {
            // Single element or already-stringified representation.
            String raw = expr.toString().trim();
            // Strip outer braces from inline array literals like {"a","b"}.
            if (raw.startsWith("{") && raw.endsWith("}")) {
                raw = raw.substring(1, raw.length() - 1).trim();
                if (raw.isEmpty()) return result;
                for (String part : raw.split(",")) {
                    String s = unquote(part.trim());
                    if (!s.isEmpty()) result.add(s);
                }
            } else {
                String s = unquote(raw);
                if (!s.isEmpty()) result.add(s);
            }
        }
        return result;
    }

    /**
     * Scan type declarations in a compilation unit for @ProveKitRefuse
     * and emit refusal-memento IR records.
     */
    private static void extractRefusals(
            CompilationUnitTree unit,
            List<Jcs.Json> entries) {
        for (var member : unit.getTypeDecls()) {
            if (member instanceof ClassTree classDecl) {
                scanClassForRefusals(classDecl, entries);
            }
        }
    }

    private static void scanClassForRefusals(ClassTree classDecl, List<Jcs.Json> entries) {
        // Check annotations on this class itself.
        for (AnnotationTree ann : classDecl.getModifiers().getAnnotations()) {
            String annName = ann.getAnnotationType().toString();
            if (!annName.equals("ProveKitRefuse") && !annName.endsWith(".ProveKitRefuse")) continue;
            parseRefuseAnnotation(ann).ifPresent(rb -> entries.add(Jcs.object(
                "concept", Jcs.string(rb.concept()),
                "kind", Jcs.string("refusal-memento"),
                "reason", Jcs.string(rb.reason()),
                "surface", Jcs.string(rb.surface()),
                "target_language", Jcs.string("java"),
                "would_close_with_cluster", Jcs.string(rb.wouldCloseWithCluster())
            )));
        }
        // Recurse into nested types.
        for (var member : classDecl.getMembers()) {
            if (member instanceof ClassTree nested) {
                scanClassForRefusals(nested, entries);
            }
        }
    }

    private static Optional<RefuseBinding> parseRefuseAnnotation(AnnotationTree ann) {
        String surface = null;
        String concept = null;
        String reason = null;
        String wouldCloseWithCluster = null;
        for (ExpressionTree arg : ann.getArguments()) {
            if (!(arg instanceof AssignmentTree assign)) continue;
            String key = assign.getVariable().toString();
            String val = unquote(assign.getExpression().toString());
            switch (key) {
                case "surface" -> surface = val;
                case "concept" -> concept = val;
                case "reason" -> reason = val;
                case "wouldCloseWithCluster" -> wouldCloseWithCluster = val;
            }
        }
        if (surface != null && concept != null && reason != null && wouldCloseWithCluster != null) {
            return Optional.of(new RefuseBinding(surface, concept, reason, wouldCloseWithCluster));
        }
        return Optional.empty();
    }

    private static List<Jcs.Json> observationTagWitnesses(String source, int startLine, int endLine) {
        List<TagLine> tags = scanObservationTags(source, startLine, endLine);
        if (tags.isEmpty()) return List.of();
        String concept = tagValue(tags, "provekit-observation");
        String mode = tagValue(tags, "provekit-observation-mode");
        String term = tagValue(tags, "provekit-observation-term");
        if (concept == null || concept.isBlank()) return List.of();
        if (term == null || term.isBlank()) {
            term = concept;
        }
        Jcs.Obj extensionFields = Jcs.object(
            "concept_site_cid", stringOrNull(tagValue(tags, "provekit-concept-site-cid")),
            "contract_cid", stringOrNull(tagValue(tags, "provekit-contract-cid")),
            "emitted_concept", stringOrNull(tagValue(tags, "provekit-emitted-concept")),
            "mode", stringOrNull(mode),
            "object_fcm_cid", stringOrNull(tagValue(tags, "provekit-object-fcm-cid")),
            "observation_concept", Jcs.string(concept),
            "observation_term", Jcs.string(term),
            "policy_cid", stringOrNull(tagValue(tags, "provekit-observation-policy-cid")),
            "role", Jcs.string("observation"),
            "surface", Jcs.string("java-comment-tag")
        );
        return List.of(Jcs.object(
            "col", Jcs.integer(0),
            "confidence_basis_points", Jcs.integer(10000),
            "extension_fields", extensionFields,
            "line", Jcs.integer(tags.get(0).line()),
            "predicate", Jcs.object("args", Jcs.array(), "kind", Jcs.string("atomic"), "name", Jcs.string(term)),
            "predicate_text", Jcs.string(term),
            "role", Jcs.string("observation"),
            "source_kind", Jcs.string("native-surface")
        ));
    }

    private static List<Jcs.Json> contractTagWitnesses(String source, int fnLine, int startLine, int endLine) {
        List<Jcs.Json> witnesses = new ArrayList<>();
        for (List<TagLine> block : contractTagBlocks(scanContractTags(source, fnLine, startLine, endLine))) {
            contractTagWitness(block).ifPresent(witnesses::add);
        }
        return witnesses;
    }

    private static Optional<Jcs.Json> contractTagWitness(List<TagLine> block) {
        String payloadText = tagValue(block, "provekit-contract");
        if (payloadText == null) return Optional.empty();
        Jcs.Json payloadJson;
        try {
            payloadJson = Jcs.parse(payloadText);
        } catch (IllegalArgumentException e) {
            return Optional.empty();
        }
        if (!(payloadJson instanceof Jcs.Obj payload)) return Optional.empty();
        if (!"provekit-contract-comment-sugar".equals(payload.stringFieldOrNull("artifact_kind"))) return Optional.empty();
        if (!"1".equals(payload.stringFieldOrNull("schema_version"))) return Optional.empty();

        String role = bindContractRole(payload.stringFieldOrNull("role"));
        if (role == null) return Optional.empty();

        String conceptSiteCid = payload.stringFieldOrNull("concept_site_cid");
        String contractCid = payload.stringFieldOrNull("contract_cid");
        String localContractCid = payload.stringFieldOrNull("local_contract_cid");
        String formulaCid = payload.stringFieldOrNull("ir_formula_jcs_cid");
        String policyCid = payload.stringFieldOrNull("policy_cid");
        String sugarDictCid = payload.stringFieldOrNull("sugar_dict_cid");
        String lossRecordCid = payload.stringFieldOrNull("loss_record_cid");
        if (!validCid(conceptSiteCid) || !validCid(contractCid) || !validCid(formulaCid)
                || !validCid(policyCid) || !validCid(sugarDictCid) || !validCid(lossRecordCid)
                || (localContractCid != null && !localContractCid.isBlank() && !validCid(localContractCid))
                || !validEmittedBy(payload.get("emitted_by"))) {
            return Optional.empty();
        }

        Jcs.Json predicate = payload.get("ir_formula_jcs");
        if (predicate == null || predicate instanceof Jcs.Null || !formulaCid.equals(Jcs.cid(predicate))) {
            return Optional.empty();
        }
        String payloadCid = Jcs.cid(payload);
        String emittedPayloadCid = tagValue(block, "provekit-contract-payload-cid");
        if (emittedPayloadCid != null && (!validCid(emittedPayloadCid) || !payloadCid.equals(emittedPayloadCid))) {
            return Optional.empty();
        }
        String predicateText = payload.stringFieldOrNull("fol_text");
        if (predicateText == null) {
            return Optional.empty();
        }

        List<Jcs.Field> extensionFieldList = new ArrayList<>();
        extensionFieldList.add(new Jcs.Field("concept_site_cid", Jcs.string(conceptSiteCid)));
        extensionFieldList.add(new Jcs.Field("contract_cid", Jcs.string(contractCid)));
        extensionFieldList.add(new Jcs.Field("ir_formula_jcs_cid", Jcs.string(formulaCid)));
        if (localContractCid != null && !localContractCid.isBlank()) {
            extensionFieldList.add(new Jcs.Field("local_contract_cid", Jcs.string(localContractCid)));
        }
        extensionFieldList.add(new Jcs.Field("loss_record_cid", Jcs.string(lossRecordCid)));
        extensionFieldList.add(new Jcs.Field("payload_cid", Jcs.string(payloadCid)));
        extensionFieldList.add(new Jcs.Field("policy_cid", Jcs.string(policyCid)));
        extensionFieldList.add(new Jcs.Field("sugar_dict_cid", Jcs.string(sugarDictCid)));
        extensionFieldList.add(new Jcs.Field("surface", Jcs.string("contract-comment-sugar")));
        Jcs.Obj extensionFields = new Jcs.Obj(extensionFieldList);
        return Optional.of(Jcs.object(
            "col", Jcs.integer(0),
            "confidence_basis_points", Jcs.integer(10000),
            "extension_fields", extensionFields,
            "line", Jcs.integer(block.get(0).line()),
            "predicate", predicate,
            "predicate_text", Jcs.string(predicateText),
            "role", Jcs.string(role),
            "source_kind", Jcs.string("native-surface")
        ));
    }

    private static ConceptCitationScan conceptCitationTags(
            String source,
            String relPath,
            int fnLine,
            int startLine,
            int endLine,
            List<Jcs.Json> diagnostics) {
        List<TagLine> tags = new ArrayList<>();
        tags.addAll(scanPreMethodTags(source, fnLine));
        tags.addAll(scanObservationTags(source, startLine, endLine));
        List<Jcs.Json> citations = new ArrayList<>();
        boolean refuseRelift = false;
        int idx = 0;
        while (idx < tags.size()) {
            TagLine tag = tags.get(idx);
            if ("provekit-concept-payload-cid".equals(tag.key())) {
                conceptCitationDiag(
                    diagnostics,
                    relPath,
                    tag.line(),
                    "concept-citation:orphan-cid-line",
                    "payload CID line has no preceding payload");
                idx++;
                continue;
            }
            if (!"provekit-concept".equals(tag.key())) {
                idx++;
                continue;
            }
            String payloadCid = null;
            if (idx + 1 < tags.size()) {
                TagLine next = tags.get(idx + 1);
                if (next.line() == tag.line() + 1 && "provekit-concept-payload-cid".equals(next.key())) {
                    payloadCid = next.value();
                    idx++;
                }
            }
            ConceptCitationValidation validation = conceptCitation(
                tag.value(),
                payloadCid,
                relPath,
                tag.line(),
                diagnostics);
            if (validation.refuseRelift()) {
                refuseRelift = true;
            }
            if (validation.citation() != null) {
                citations.add(validation.citation());
            }
            idx++;
        }
        return new ConceptCitationScan(citations, refuseRelift);
    }

    private static ConceptCitationValidation conceptCitation(
            String payloadText,
            String emittedPayloadCid,
            String relPath,
            int line,
            List<Jcs.Json> diagnostics) {
        Jcs.Json payloadJson;
        try {
            payloadJson = Jcs.parse(payloadText);
        } catch (IllegalArgumentException e) {
            conceptCitationDiag(
                diagnostics,
                relPath,
                line,
                "concept-citation:malformed-json",
                "malformed JSON: " + e.getMessage());
            return ConceptCitationValidation.drop();
        }
        if (!(payloadJson instanceof Jcs.Obj payload)) {
            conceptCitationDiag(
                diagnostics,
                relPath,
                line,
                "concept-citation:malformed-json",
                "payload is not an object");
            return ConceptCitationValidation.drop();
        }

        if (!CONCEPT_CITATION_COMMENT_KIND.equals(payload.stringFieldOrNull("artifact_kind"))) {
            conceptCitationDiag(
                diagnostics,
                relPath,
                line,
                "concept-citation:unknown-schema-version",
                "wrong artifact_kind");
            return ConceptCitationValidation.drop();
        }
        if (!"1".equals(payload.stringFieldOrNull("schema_version"))) {
            conceptCitationDiag(
                diagnostics,
                relPath,
                line,
                "concept-citation:unknown-schema-version",
                "unknown schema_version");
            return ConceptCitationValidation.drop();
        }
        if (!validConceptEmittedBy(payload.get("emitted_by"))) {
            conceptCitationDiag(
                diagnostics,
                relPath,
                line,
                "concept-citation:malformed-cid",
                "malformed emitted_by");
            return ConceptCitationValidation.drop();
        }

        String operationKind = payload.stringFieldOrNull("operation_kind");
        if (operationKind == null || operationKind.isBlank()) {
            conceptCitationDiag(
                diagnostics,
                relPath,
                line,
                "concept-citation:malformed-json",
                "missing operation_kind");
            return ConceptCitationValidation.drop();
        }
        Jcs.Json termPosition = payload.get("term_position");
        if (!validTermPosition(termPosition)) {
            conceptCitationDiag(
                diagnostics,
                relPath,
                line,
                "concept-citation:malformed-json",
                "malformed term_position");
            return ConceptCitationValidation.drop();
        }

        String[] cidFields = {
            "args_jcs_cid",
            "concept_cid",
            "concept_site_cid",
            "loss_record_cid",
            "shape_cid",
            "sugar_dict_cid"
        };
        for (String key : cidFields) {
            if (!validCid(payload.stringFieldOrNull(key))) {
                conceptCitationDiag(
                    diagnostics,
                    relPath,
                    line,
                    "concept-citation:malformed-cid",
                    "malformed " + key);
                return ConceptCitationValidation.drop();
            }
        }
        for (String key : List.of("callsite_cid", "policy_cid")) {
            Jcs.Json value = payload.get(key);
            if (value != null && (!(value instanceof Jcs.Str s) || !validCid(s.value()))) {
                conceptCitationDiag(
                    diagnostics,
                    relPath,
                    line,
                    "concept-citation:malformed-cid",
                    "malformed " + key);
                return ConceptCitationValidation.drop();
            }
        }
        if (emittedPayloadCid == null) {
            conceptCitationDiag(
                diagnostics,
                relPath,
                line,
                "concept-citation:payload-cid-mismatch",
                "missing payload CID");
            return ConceptCitationValidation.drop();
        }
        if (!validCid(emittedPayloadCid)) {
            conceptCitationDiag(
                diagnostics,
                relPath,
                line,
                "concept-citation:malformed-cid",
                "malformed payload CID");
            return ConceptCitationValidation.drop();
        }
        String payloadCid = Jcs.cid(payload);
        if (!payloadCid.equals(emittedPayloadCid)) {
            conceptCitationDiag(
                diagnostics,
                relPath,
                line,
                "concept-citation:payload-cid-mismatch",
                "payload CID mismatch");
            return ConceptCitationValidation.drop();
        }

        Jcs.Json argsJcs = payload.get("args_jcs");
        if (argsJcs != null) {
            if (!(argsJcs instanceof Jcs.Arr)) {
                conceptCitationDiag(
                    diagnostics,
                    relPath,
                    line,
                    "concept-citation:malformed-json",
                    "malformed args_jcs");
                return ConceptCitationValidation.drop();
            }
            if (!Jcs.cid(argsJcs).equals(payload.stringField("args_jcs_cid"))) {
                conceptCitationDiag(
                    diagnostics,
                    relPath,
                    line,
                    "concept-citation:args-cid-mismatch",
                    "args CID mismatch");
                return ConceptCitationValidation.drop();
            }
        }

        Map<String, CatalogEntry> catalog = conceptShapeCatalog();
        if (catalog != null) {
            CatalogEntry catalogEntry = catalog.get(payload.stringField("concept_cid"));
            if (catalogEntry == null) {
                conceptCitationDiag(
                    diagnostics,
                    relPath,
                    line,
                    "concept-citation:unknown-concept",
                    "concept not in local catalog");
                return ConceptCitationValidation.drop();
            }
            if (!catalogEntry.shapeCid().equals(payload.stringField("shape_cid"))) {
                conceptCitationDiag(
                    diagnostics,
                    relPath,
                    line,
                    "concept-citation:shape-mismatch",
                    "shape CID mismatch");
                return ConceptCitationValidation.refuse();
            }
            if (!catalogEntry.operationKind().equals(operationKind)) {
                conceptCitationDiag(
                    diagnostics,
                    relPath,
                    line,
                    "concept-citation:operation-kind-mismatch",
                    "operation_kind mismatch");
                return ConceptCitationValidation.refuse();
            }
        }

        List<Jcs.Field> extensionFieldList = new ArrayList<>();
        extensionFieldList.add(new Jcs.Field("args_jcs_cid", Jcs.string(payload.stringField("args_jcs_cid"))));
        extensionFieldList.add(new Jcs.Field("concept_site_cid", Jcs.string(payload.stringField("concept_site_cid"))));
        extensionFieldList.add(new Jcs.Field("loss_record_cid", Jcs.string(payload.stringField("loss_record_cid"))));
        extensionFieldList.add(new Jcs.Field("payload_cid", Jcs.string(payloadCid)));
        extensionFieldList.add(new Jcs.Field("shape_cid", Jcs.string(payload.stringField("shape_cid"))));
        extensionFieldList.add(new Jcs.Field("sugar_dict_cid", Jcs.string(payload.stringField("sugar_dict_cid"))));
        extensionFieldList.add(new Jcs.Field("surface", Jcs.string("concept-citation-comment-sugar")));
        if (payload.get("callsite_cid") instanceof Jcs.Str callsiteCid) {
            extensionFieldList.add(new Jcs.Field("callsite_cid", Jcs.string(callsiteCid.value())));
        }
        if (payload.get("policy_cid") instanceof Jcs.Str policyCid) {
            extensionFieldList.add(new Jcs.Field("policy_cid", Jcs.string(policyCid.value())));
        }
        if (argsJcs != null) {
            extensionFieldList.add(new Jcs.Field("args_jcs", argsJcs));
        }

        return new ConceptCitationValidation(
            Jcs.object(
                "args_jcs_cid", Jcs.string(payload.stringField("args_jcs_cid")),
                "artifact_kind", Jcs.string(CONCEPT_CITATION_COMMENT_KIND),
                "col", Jcs.integer(0),
                "confidence_basis_points", Jcs.integer(10000),
                "concept_cid", Jcs.string(payload.stringField("concept_cid")),
                "extension_fields", new Jcs.Obj(extensionFieldList),
                "line", Jcs.integer(line),
                "operation_kind", Jcs.string(operationKind),
                "shape_cid", Jcs.string(payload.stringField("shape_cid")),
                "source_kind", Jcs.string("native-surface"),
                "term_position", termPosition
            ),
            false);
    }

    private static boolean validConceptEmittedBy(Jcs.Json value) {
        if (!(value instanceof Jcs.Obj emittedByObj)) return false;
        String kitId = emittedByObj.stringFieldOrNull("kit_id");
        String targetLibraryTag = emittedByObj.stringFieldOrNull("target_library_tag");
        return validCid(emittedByObj.stringFieldOrNull("kit_cid"))
            && nonBlank(kitId)
            && nonBlank(emittedByObj.stringFieldOrNull("kit_kind"))
            && nonBlank(emittedByObj.stringFieldOrNull("target_language"))
            && (targetLibraryTag == null || !targetLibraryTag.isBlank());
    }

    private static boolean validTermPosition(Jcs.Json value) {
        if (!(value instanceof Jcs.Arr arr)) return false;
        for (Jcs.Json item : arr.values()) {
            if (!(item instanceof Jcs.Num number) || number.value() < 0) {
                return false;
            }
        }
        return true;
    }

    private static void conceptCitationDiag(
            List<Jcs.Json> diagnostics,
            String relPath,
            int line,
            String kind,
            String message) {
        diagnostics.add(Jcs.object(
            "kind", Jcs.string(kind),
            "line", Jcs.integer(line),
            "message", Jcs.string(message),
            "path", Jcs.string(relPath)
        ));
    }

    private static Map<String, CatalogEntry> conceptShapeCatalog() {
        Path root = repoRoot();
        if (root == null) return null;
        Path indexPath = root.resolve("menagerie/concept-shapes/catalog/index.json");
        Jcs.Json indexJson;
        try {
            indexJson = Jcs.parse(Files.readString(indexPath, StandardCharsets.UTF_8));
        } catch (IOException | IllegalArgumentException e) {
            return null;
        }
        if (!(indexJson instanceof Jcs.Obj indexObj)) return null;
        Jcs.Json entriesJson = indexObj.get("entries");
        if (!(entriesJson instanceof Jcs.Obj entriesObj)) return null;
        Map<String, CatalogEntry> catalog = new HashMap<>();
        Path catalogRoot = indexPath.getParent();
        for (Jcs.Field field : entriesObj.fields()) {
            String cid = field.key();
            if (!validCid(cid) || !(field.value() instanceof Jcs.Obj meta)) continue;
            if (!"algorithm".equals(meta.stringFieldOrNull("kind"))) continue;
            String name = meta.stringFieldOrNull("name");
            String relative = meta.stringFieldOrNull("path");
            if (name == null || !name.startsWith("concept:") || relative == null || relative.isBlank()) continue;
            try {
                Jcs.Json documentJson = Jcs.parse(Files.readString(catalogRoot.resolve(relative), StandardCharsets.UTF_8));
                if (!(documentJson instanceof Jcs.Obj document)) continue;
                String shapeCid = document.stringFieldOrNull("cid");
                Jcs.Json mementoJson = document.get("memento");
                if (!validCid(shapeCid) || !(mementoJson instanceof Jcs.Obj memento)) continue;
                String operationKind = catalogOperationKind(name, memento);
                if (operationKind != null && !operationKind.isBlank()) {
                    catalog.put(cid, new CatalogEntry(name, shapeCid, operationKind));
                }
            } catch (IOException | IllegalArgumentException ignored) {
            }
        }
        return catalog;
    }

    private static String conceptCidForName(String conceptName) {
        Map<String, CatalogEntry> catalog = conceptShapeCatalog();
        if (catalog == null) return null;
        for (Map.Entry<String, CatalogEntry> entry : catalog.entrySet()) {
            if (conceptName.equals(entry.getValue().name())) {
                return entry.getKey();
            }
        }
        return null;
    }

    private static String catalogOperationKind(String name, Jcs.Obj memento) {
        Jcs.Json post = memento.get("post");
        if (post instanceof Jcs.Obj postObj) {
            String operator = postObj.stringFieldOrNull("operator");
            if (operator != null && !operator.isBlank()) {
                return operator;
            }
        }
        return name.startsWith("concept:") ? name.substring("concept:".length()) : null;
    }

    private static Path repoRoot() {
        List<Path> candidates = new ArrayList<>();
        Path cwd = Path.of("").toAbsolutePath().normalize();
        candidates.add(cwd);
        candidates.addAll(parentPaths(cwd));
        try {
            Path codeLocation = Path.of(JavaBindLifter.class.getProtectionDomain().getCodeSource().getLocation().toURI())
                .toAbsolutePath()
                .normalize();
            candidates.add(codeLocation);
            candidates.addAll(parentPaths(codeLocation));
        } catch (Exception ignored) {
        }
        for (Path candidate : candidates) {
            if (Files.exists(candidate.resolve("menagerie/concept-shapes/catalog/index.json"))) {
                return candidate;
            }
        }
        return null;
    }

    private static List<Path> parentPaths(Path path) {
        List<Path> parents = new ArrayList<>();
        Path cursor = path.getParent();
        while (cursor != null) {
            parents.add(cursor);
            cursor = cursor.getParent();
        }
        return parents;
    }

    private static List<List<TagLine>> contractTagBlocks(List<TagLine> tags) {
        List<List<TagLine>> blocks = new ArrayList<>();
        List<TagLine> current = null;
        for (TagLine tag : tags) {
            if ("provekit-contract".equals(tag.key())) {
                current = new ArrayList<>();
                current.add(tag);
                blocks.add(current);
                continue;
            }
            if (current != null && tag.key().startsWith("provekit-contract-")) {
                current.add(tag);
            }
        }
        return blocks;
    }

    private static List<TagLine> scanContractTags(String source, int fnLine, int startLine, int endLine) {
        List<TagLine> tags = new ArrayList<>();
        tags.addAll(scanPreMethodTags(source, fnLine));
        tags.addAll(scanObservationTags(source, startLine, endLine));
        return tags;
    }

    private static List<TagLine> scanPreMethodTags(String source, int fnLine) {
        if (fnLine <= 1) return List.of();
        String[] lines = source.split("\n", -1);
        int cursor = Math.min(lines.length - 1, fnLine - 2);
        while (cursor >= 0 && isMethodHeaderCompanionLine(lines[cursor].stripLeading())) {
            cursor--;
        }
        int start = cursor + 1;
        List<TagLine> tags = new ArrayList<>();
        for (int idx = start; idx < fnLine - 1 && idx < lines.length; idx++) {
            parseProvekitTagLine(lines[idx], idx + 1).ifPresent(tags::add);
        }
        return tags;
    }

    private static boolean isMethodHeaderCompanionLine(String line) {
        return line.isEmpty()
            || line.startsWith("//")
            || line.startsWith("@")
            || line.startsWith("/*")
            || line.startsWith("*")
            || line.startsWith("*/");
    }

    private static List<TagLine> scanObservationTags(String source, int startLine, int endLine) {
        if (startLine <= 0 || endLine < startLine) return List.of();
        String[] lines = source.split("\n", -1);
        int start = Math.max(0, startLine - 1);
        int end = Math.min(lines.length, endLine);
        List<TagLine> tags = new ArrayList<>();
        for (int idx = start; idx < end; idx++) {
            parseProvekitTagLine(lines[idx], idx + 1).ifPresent(tags::add);
        }
        return tags;
    }

    private static Optional<TagLine> parseProvekitTagLine(String line, int lineNumber) {
        String stripped = line.stripLeading();
        if (!stripped.startsWith("// provekit-")) return Optional.empty();
        int colon = stripped.indexOf(':');
        if (colon < 0) return Optional.empty();
        String key = stripped.substring("// ".length(), colon).trim();
        String value = stripped.substring(colon + 1).trim();
        if (!key.startsWith("provekit-")) return Optional.empty();
        return Optional.of(new TagLine(lineNumber, key, value));
    }

    private static String tagValue(List<TagLine> tags, String key) {
        for (TagLine tag : tags) {
            if (tag.key().equals(key)) return tag.value();
        }
        return null;
    }

    private static Jcs.Json stringOrNull(String value) {
        return value == null || value.isBlank() ? Jcs.nullValue() : Jcs.string(value);
    }

    private static String bindContractRole(String payloadRole) {
        return switch (payloadRole == null ? "" : payloadRole) {
            case "pre" -> "pre";
            case "post" -> "post";
            case "invariant" -> "inv";
            case "throws" -> "throws";
            case "observation" -> "observation";
            default -> null;
        };
    }

    private static boolean validEmittedBy(Jcs.Json emittedBy) {
        if (!(emittedBy instanceof Jcs.Obj emittedByObj)) return false;
        return validCid(emittedByObj.stringFieldOrNull("kit_cid"))
            && nonBlank(emittedByObj.stringFieldOrNull("kit_kind"))
            && nonBlank(emittedByObj.stringFieldOrNull("target_language"));
    }

    private static boolean nonBlank(String value) {
        return value != null && !value.isBlank();
    }

    private static boolean validCid(String cid) {
        if (cid == null || !cid.startsWith("blake3-512:") || cid.length() != "blake3-512:".length() + 128) {
            return false;
        }
        for (int i = "blake3-512:".length(); i < cid.length(); i++) {
            char ch = cid.charAt(i);
            if (!((ch >= '0' && ch <= '9') || (ch >= 'a' && ch <= 'f'))) {
                return false;
            }
        }
        return true;
    }

    // ---- Helpers -----------------------------------------------------------

    private static int lineOf(String source, int offset) {
        if (offset <= 0) return 1;
        int line = 1;
        int end = Math.min(offset, source.length());
        for (int i = 0; i < end; i++) if (source.charAt(i) == '\n') line++;
        return line;
    }

    private static int columnOf(String source, int offset) {
        if (offset <= 0) return 0;
        int col = 0;
        int cursor = Math.min(offset, source.length()) - 1;
        for (int i = cursor; i >= 0; i--) {
            if (source.charAt(i) == '\n') break;
            col++;
        }
        return col;
    }

    private static String typeName(TypeMirror t) {
        if (t == null) return "?";
        if (t.getKind() == TypeKind.VOID) return "()";
        return t.toString();
    }

    private static Jcs.Obj diag(String kind, String message) {
        return Jcs.object("kind", Jcs.string(kind), "message", Jcs.string(message));
    }

    private static Jcs.Obj withField(Jcs.Obj base, String key, Jcs.Json value) {
        List<Jcs.Field> fields = new ArrayList<>(base.fields());
        fields.add(new Jcs.Field(key, value));
        return new Jcs.Obj(fields);
    }

    public record Result(List<Jcs.Json> entries, List<Jcs.Json> diagnostics) {
        public Jcs.Obj toJson() {
            return Jcs.object(
                "diagnostics", Jcs.array(diagnostics),
                "ir", Jcs.array(entries),
                "kind", Jcs.string("ir-document")
            );
        }
    }

    private record ConceptCitationScan(List<Jcs.Json> citations, boolean refuseRelift) {}

    private record ConceptCitationValidation(Jcs.Json citation, boolean refuseRelift) {
        static ConceptCitationValidation drop() {
            return new ConceptCitationValidation(null, false);
        }

        static ConceptCitationValidation refuse() {
            return new ConceptCitationValidation(null, true);
        }
    }

    private record ShapeResult(Jcs.Json shape, List<Jcs.Json> operandBindings) {
        static ShapeResult empty() {
            return new ShapeResult(Jcs.object(), List.of());
        }
    }

    private record CatalogEntry(String name, String shapeCid, String operationKind) {}

    private record TagLine(int line, String key, String value) {}

    private static final class JavaFileSource extends SimpleJavaFileObject {
        private final String source;
        JavaFileSource(String path, String source) {
            super(URI.create("string:///" + path.replace('\\', '/')), javax.tools.JavaFileObject.Kind.SOURCE);
            this.source = source;
        }
        @Override public CharSequence getCharContent(boolean ignoreEncodingErrors) { return source; }
    }
}
