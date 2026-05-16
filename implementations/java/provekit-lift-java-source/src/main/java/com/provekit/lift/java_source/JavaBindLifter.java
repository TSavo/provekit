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
// Counterpart: `implementations/rust/provekit-walk/src/bin/walk_rpc.rs::bind_lift`
// (Rust does the same walk over `syn::ItemFn` and emits identical-shape records).

package com.provekit.lift.java_source;

import com.provekit.ir.Jcs;
import com.sun.source.tree.AssignmentTree;
import com.sun.source.tree.BinaryTree;
import com.sun.source.tree.BlockTree;
import com.sun.source.tree.CompilationUnitTree;
import com.sun.source.tree.CompoundAssignmentTree;
import com.sun.source.tree.DoWhileLoopTree;
import com.sun.source.tree.EnhancedForLoopTree;
import com.sun.source.tree.ExpressionStatementTree;
import com.sun.source.tree.ForLoopTree;
import com.sun.source.tree.IfTree;
import com.sun.source.tree.MethodInvocationTree;
import com.sun.source.tree.MethodTree;
import com.sun.source.tree.NewClassTree;
import com.sun.source.tree.ReturnTree;
import com.sun.source.tree.StatementTree;
import com.sun.source.tree.Tree;
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
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
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
            source = Files.readString(javaFile);
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
            new MethodScanner(trees, rel, source, entries).scan(unit, null);
        }
    }

    /** Walks methods of a compilation unit and pushes one bind-lift-entry per method. */
    private static final class MethodScanner extends TreePathScanner<Void, Void> {
        private final Trees trees;
        private final String rel;
        private final String source;
        private final List<Jcs.Json> entries;

        MethodScanner(Trees trees, String rel, String source, List<Jcs.Json> entries) {
            this.trees = trees;
            this.rel = rel;
            this.source = source;
            this.entries = entries;
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

            Jcs.Obj termShape = shapeOfStatement(method.getBody());
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

            Jcs.Obj entry = Jcs.object(
                "attr_post", Jcs.nullValue(),
                "attr_pre", Jcs.nullValue(),
                "concept_annotation", conceptAnnotation == null ? Jcs.nullValue() : Jcs.string(conceptAnnotation),
                "file", Jcs.string(rel),
                "fn_line", Jcs.integer(fnLine),
                "fn_name", Jcs.string(fnName),
                "kind", Jcs.string("bind-lift-entry"),
                "param_names", Jcs.array(paramNames),
                "param_types", Jcs.array(paramTypes),
                "return_type", Jcs.string(returnType),
                "term_shape", termShape,
                "term_shape_cid", Jcs.string(termShapeCid),
                "witnesses", Jcs.array(surfaceWitnesses)
            );
            entries.add(entry);
            return super.visitMethod(method, unused);
        }
    }

    // ---- Term-shape mapping ------------------------------------------------
    //
    // Maps javac AST nodes to the language-neutral term-shape kinds defined
    // in `2026-05-13-bind-ir-lift-result.md` §2. Identical structurally to
    // `walk_rpc.rs::term_shape_for_fn` and `cmd_bind::TermShape::from_fn` on
    // the Rust side; same shape, same JCS bytes, same shape_cid.

    private static Jcs.Obj shapeOfStatement(StatementTree stmt) {
        if (stmt == null) return Jcs.object("kind", Jcs.string("opaque"));
        if (stmt instanceof BlockTree b) {
            List<Jcs.Json> ss = new ArrayList<>();
            for (StatementTree s : b.getStatements()) ss.add(shapeOfStatement(s));
            return Jcs.object("kind", Jcs.string("body"), "stmts", Jcs.array(ss));
        }
        if (stmt instanceof IfTree t) {
            Jcs.Obj o = Jcs.object(
                "cond", shapeOfExpression(t.getCondition()),
                "kind", Jcs.string("if"),
                "then", shapeOfStatement(t.getThenStatement())
            );
            if (t.getElseStatement() != null) {
                o = withField(o, "else", shapeOfStatement(t.getElseStatement()));
            }
            return o;
        }
        if (stmt instanceof WhileLoopTree t) {
            return Jcs.object(
                "body", shapeOfStatement(t.getStatement()),
                "cond", shapeOfExpression(t.getCondition()),
                "kind", Jcs.string("while")
            );
        }
        if (stmt instanceof DoWhileLoopTree t) {
            return Jcs.object(
                "body", shapeOfStatement(t.getStatement()),
                "cond", shapeOfExpression(t.getCondition()),
                "kind", Jcs.string("while")
            );
        }
        if (stmt instanceof ForLoopTree t) {
            return Jcs.object(
                "body", shapeOfStatement(t.getStatement()),
                "kind", Jcs.string("for")
            );
        }
        if (stmt instanceof EnhancedForLoopTree t) {
            return Jcs.object(
                "body", shapeOfStatement(t.getStatement()),
                "kind", Jcs.string("for")
            );
        }
        if (stmt instanceof ReturnTree) return Jcs.object("kind", Jcs.string("exit"));
        if (stmt instanceof com.sun.source.tree.BreakTree) return Jcs.object("kind", Jcs.string("exit"));
        if (stmt instanceof com.sun.source.tree.ContinueTree) return Jcs.object("kind", Jcs.string("exit"));
        if (stmt instanceof VariableTree) return Jcs.object("kind", Jcs.string("let"));
        if (stmt instanceof ExpressionStatementTree es) return shapeOfExpression(es.getExpression());
        return Jcs.object("kind", Jcs.string("opaque"));
    }

    private static Jcs.Obj shapeOfExpression(Tree expr) {
        if (expr == null) return Jcs.object("kind", Jcs.string("opaque"));
        if (expr instanceof AssignmentTree) return Jcs.object("kind", Jcs.string("assign"));
        if (expr instanceof CompoundAssignmentTree) return Jcs.object("kind", Jcs.string("assign"));
        if (expr instanceof BinaryTree b) {
            String op = switch (b.getKind()) {
                case PLUS -> "+";
                case MINUS -> "-";
                case MULTIPLY -> "*";
                case DIVIDE -> "/";
                case REMAINDER -> "%";
                case EQUAL_TO -> "==";
                case NOT_EQUAL_TO -> "!=";
                case LESS_THAN -> "<";
                case LESS_THAN_EQUAL -> "<=";
                case GREATER_THAN -> ">";
                case GREATER_THAN_EQUAL -> ">=";
                default -> "opaque-op";
            };
            boolean isRel = switch (op) {
                case "==", "!=", "<", "<=", ">", ">=" -> true;
                default -> false;
            };
            return Jcs.object("kind", Jcs.string(isRel ? "rel" : "bin"), "op", Jcs.string(op));
        }
        if (expr instanceof MethodInvocationTree) return Jcs.object("kind", Jcs.string("call"));
        if (expr instanceof NewClassTree) return Jcs.object("kind", Jcs.string("call"));
        return Jcs.object("kind", Jcs.string("opaque"));
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
