package com.provekit.lift.java_source;

import com.provekit.ir.Jcs;
import com.sun.source.tree.ArrayAccessTree;
import com.sun.source.tree.AssignmentTree;
import com.sun.source.tree.BinaryTree;
import com.sun.source.tree.BlockTree;
import com.sun.source.tree.BreakTree;
import com.sun.source.tree.CaseTree;
import com.sun.source.tree.ClassTree;
import com.sun.source.tree.CompilationUnitTree;
import com.sun.source.tree.CompoundAssignmentTree;
import com.sun.source.tree.ConditionalExpressionTree;
import com.sun.source.tree.ContinueTree;
import com.sun.source.tree.DoWhileLoopTree;
import com.sun.source.tree.EnhancedForLoopTree;
import com.sun.source.tree.ExpressionStatementTree;
import com.sun.source.tree.ExpressionTree;
import com.sun.source.tree.ForLoopTree;
import com.sun.source.tree.IdentifierTree;
import com.sun.source.tree.IfTree;
import com.sun.source.tree.LiteralTree;
import com.sun.source.tree.MemberSelectTree;
import com.sun.source.tree.MethodInvocationTree;
import com.sun.source.tree.MethodTree;
import com.sun.source.tree.NewClassTree;
import com.sun.source.tree.ParenthesizedTree;
import com.sun.source.tree.ReturnTree;
import com.sun.source.tree.StatementTree;
import com.sun.source.tree.ThrowTree;
import com.sun.source.tree.Tree;
import com.sun.source.tree.TypeCastTree;
import com.sun.source.tree.UnaryTree;
import com.sun.source.tree.VariableTree;
import com.sun.source.tree.WhileLoopTree;
import com.sun.source.util.JavacTask;
import com.sun.source.util.SourcePositions;
import com.sun.source.util.TreePath;
import com.sun.source.util.TreePathScanner;
import com.sun.source.util.Trees;
import java.io.IOException;
import java.net.URI;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import javax.lang.model.element.Element;
import javax.lang.model.element.ElementKind;
import javax.lang.model.element.ExecutableElement;
import javax.lang.model.element.Modifier;
import javax.lang.model.element.TypeElement;
import javax.lang.model.element.VariableElement;
import javax.lang.model.type.TypeKind;
import javax.lang.model.type.TypeMirror;
import javax.lang.model.util.Elements;
import javax.lang.model.util.Types;
import javax.tools.Diagnostic;
import javax.tools.DiagnosticCollector;
import javax.tools.JavaCompiler;
import javax.tools.SimpleJavaFileObject;
import javax.tools.ToolProvider;

public final class JavaSourceLifter {
    private static final String PANIC_FREEDOM_EFFECT_KIND = "concept:panic-freedom";
    private static final String RUNTIME_FAILURE_SITE_CONCEPT = "concept:panic-freedom.leaf.runtime-failure-site";

    public LiftResult liftSource(String path, String source) {
        List<Jcs.Json> declarations = new ArrayList<>();
        List<Jcs.Json> diagnosticsJson = new ArrayList<>();
        List<Refusal> refusals = new ArrayList<>();

        Parsed parsed = parse(path, source, diagnosticsJson, refusals);
        if (parsed != null) {
            MethodScanner scanner = new MethodScanner(parsed, path, declarations, refusals);
            for (CompilationUnitTree unit : parsed.units()) {
                scanner.scan(unit, null);
            }
        }

        Jcs.Obj operational = wrapSeqFromContracts(declarations, 0);
        Jcs.Obj sourceUnitTerm = ctor("java:source-unit", Jcs.string(source), operational);
        Jcs.Obj sourceUnitContract = functionContract(
            "<source-unit:" + path + ">",
            List.of(),
            List.of(),
            sort("Int"),
            eq(var("result"), sourceUnitTerm),
            List.of(),
            List.of(),
            path,
            1,
            1
        );
        declarations.add(0, sourceUnitContract);

        return new LiftResult(declarations, diagnosticsJson, refusals, sourceUnitTerm);
    }

    public LiftResult liftPaths(String workspaceRoot, List<String> sourcePaths) {
        Path root = Path.of(workspaceRoot).toAbsolutePath().normalize();
        List<Jcs.Json> declarations = new ArrayList<>();
        List<Jcs.Json> diagnostics = new ArrayList<>();
        List<Refusal> refusals = new ArrayList<>();
        for (String sourcePath : sourcePaths) {
            Path resolved = root.resolve(sourcePath).toAbsolutePath().normalize();
            if (!resolved.equals(root) && !resolved.startsWith(root)) {
                refusals.add(new Refusal("path-traversal", null, null,
                    "path '" + sourcePath + "' escapes workspace root '" + root + "'"));
                continue;
            }
            try {
                if (Files.isDirectory(resolved)) {
                    try (var stream = Files.walk(resolved)) {
                        for (Path javaFile : stream.filter(p -> p.toString().endsWith(".java")).sorted().toList()) {
                            LiftResult file = liftSource(root.relativize(javaFile).toString(), Files.readString(javaFile));
                            declarations.addAll(file.declarations());
                            diagnostics.addAll(file.diagnostics());
                            refusals.addAll(file.refusals());
                        }
                    }
                } else if (Files.exists(resolved)) {
                    LiftResult file = liftSource(root.relativize(resolved).toString(), Files.readString(resolved));
                    declarations.addAll(file.declarations());
                    diagnostics.addAll(file.diagnostics());
                    refusals.addAll(file.refusals());
                } else {
                    diagnostics.add(diag("warning", "path not found: " + resolved));
                }
            } catch (IOException e) {
                diagnostics.add(diag("error", "read failed for " + resolved + ": " + e.getMessage()));
                refusals.add(new Refusal("io-error", null, null, "cannot read '" + resolved + "'"));
            }
        }
        Jcs.Obj sourceUnitTerm = declarations.isEmpty() ? ctor("java:skip", intConst(0)) : wrapSeqFromContracts(declarations, 0);
        return new LiftResult(declarations, diagnostics, refusals, sourceUnitTerm);
    }

    private static Parsed parse(String path, String source, List<Jcs.Json> diagnosticsJson, List<Refusal> refusals) {
        JavaCompiler compiler = ToolProvider.getSystemJavaCompiler();
        if (compiler == null) {
            refusals.add(new Refusal("compiler-unavailable", null, null, "JDK compiler API is not available"));
            return null;
        }
        DiagnosticCollector<javax.tools.JavaFileObject> diagnostics = new DiagnosticCollector<>();
        JavaSourceFile file = new JavaSourceFile(path, source);
        List<String> options = List.of("-proc:none", "-Xlint:none");
        JavacTask task = (JavacTask) compiler.getTask(null, null, diagnostics, options, null, List.of(file));
        try {
            Iterable<? extends CompilationUnitTree> parsed = task.parse();
            List<CompilationUnitTree> units = new ArrayList<>();
            parsed.forEach(units::add);
            try {
                task.analyze();
            } catch (Throwable ignored) {
                // Keep parse trees and explicit refusals even when attribution reports errors.
            }
            for (Diagnostic<?> d : diagnostics.getDiagnostics()) {
                diagnosticsJson.add(diag(d.getKind().name().toLowerCase(Locale.ROOT), d.getMessage(Locale.ROOT)));
            }
            return new Parsed(task, Trees.instance(task), task.getTypes(), task.getElements(), units);
        } catch (IOException | RuntimeException e) {
            refusals.add(new Refusal("parse-error", null, null, e.getMessage()));
            return null;
        }
    }

    private record Parsed(JavacTask task, Trees trees, Types types, Elements elements, List<CompilationUnitTree> units) {}

    public record LiftResult(List<Jcs.Json> declarations, List<Jcs.Json> diagnostics, List<Refusal> refusals, Jcs.Obj sourceUnitTerm) {
        public Jcs.Obj toJson() {
            return Jcs.object(
                "kind", Jcs.string("ir-document"),
                "ir", Jcs.array(declarations),
                "callEdges", Jcs.array(),
                "diagnostics", Jcs.array(diagnostics),
                "opacityReport", Jcs.array(),
                "refusals", Jcs.array(refusals.stream().map(Refusal::toJson).toList())
            );
        }
    }

    public record Refusal(
        String kind,
        String function,
        Integer line,
        String reason) {
        public Jcs.Obj toJson() {
            List<Jcs.Field> fields = new ArrayList<>();
            fields.add(new Jcs.Field("kind", Jcs.string(kind)));
            fields.add(new Jcs.Field("function", function == null ? Jcs.nullValue() : Jcs.string(function)));
            fields.add(new Jcs.Field("line", line == null ? Jcs.nullValue() : Jcs.integer(line)));
            fields.add(new Jcs.Field("reason", Jcs.string(reason)));
            return new Jcs.Obj(fields);
        }
    }

    private static final class JavaSourceFile extends SimpleJavaFileObject {
        private final String source;

        JavaSourceFile(String path, String source) {
            super(URI.create("string:///" + path.replace('\\', '/')), javax.tools.JavaFileObject.Kind.SOURCE);
            this.source = source;
        }

        @Override
        public CharSequence getCharContent(boolean ignoreEncodingErrors) {
            return source;
        }
    }

    private static final class MethodScanner extends TreePathScanner<Void, Void> {
        private final Parsed parsed;
        private final String path;
        private final List<Jcs.Json> declarations;
        private final List<Refusal> refusals;

        MethodScanner(Parsed parsed, String path, List<Jcs.Json> declarations, List<Refusal> refusals) {
            this.parsed = parsed;
            this.path = path;
            this.declarations = declarations;
            this.refusals = refusals;
        }

        @Override
        public Void visitMethod(MethodTree method, Void unused) {
            TreePath methodPath = getCurrentPath();
            Element element = parsed.trees().getElement(methodPath);
            if (!(element instanceof ExecutableElement executable) || method.getBody() == null) {
                return null;
            }
            if (method.getReturnType() == null || executable.getSimpleName().contentEquals("<init>")) {
                return null;
            }
            String fnName = functionName(executable, parsed.types(), parsed.elements());
            int line = lineOf(parsed.trees(), getCurrentPath().getCompilationUnit(), method);
            if (executable.isVarArgs()) {
                refusals.add(new Refusal("unsupported-varargs", fnName, line, "varargs methods are not in the java-source v1 slice"));
                return null;
            }
            if (!method.getTypeParameters().isEmpty()) {
                refusals.add(new Refusal("unsupported-generics", fnName, line, "generic methods are not in the java-source v1 slice"));
                return null;
            }
            if (executable.getReturnType().getKind() == TypeKind.VOID) {
                refusals.add(new Refusal("unsupported-return-sort", fnName, line, "java-source v1 emits value-returning function-contract mementos"));
                return null;
            }
            try {
                Emitter emitter = new Emitter(parsed, getCurrentPath().getCompilationUnit(), path, fnName);
                for (VariableElement param : executable.getParameters()) {
                    emitter.addLocal(param.getSimpleName().toString());
                }
                Jcs.Json body = emitter.emitStatement(new TreePath(methodPath, method.getBody()));
                Jcs.Json postValue = singleReturnExpression(methodPath, method.getBody(), emitter);
                if (postValue == null) postValue = body;
                List<String> formals = executable.getParameters().stream().map(p -> p.getSimpleName().toString()).toList();
                List<Jcs.Json> formalSorts = executable.getParameters().stream().map(p -> (Jcs.Json) sortFor(p.asType())).toList();
                Jcs.Obj contract = functionContract(
                    fnName,
                    formals,
                    formalSorts,
                    sortFor(executable.getReturnType()),
                    // Body-derived postcondition: `result == <body value-expr>`.
                    // The result var MUST be `result` (not `return_value`): the
                    // verification spine's body_discharge::CatalogResolver
                    // (RESULT_VAR = "result", #1436/#1440) substitutes a
                    // harvested call's arg into the matching formal of a
                    // `result == ...` post. With `return_value` the resolver
                    // returns None and the java callee stays uninterpreted ->
                    // Undecidable.
                    eq(var("result"), postValue),
                    emitter.effectsJson(),
                    emitter.panicLociJson(),
                    path,
                    line,
                    1
                );
                declarations.add(contract);
            } catch (RefuseException e) {
                refusals.add(new Refusal(e.kind, fnName, e.line, e.getMessage()));
            } catch (RuntimeException e) {
                refusals.add(new Refusal("analysis-error", fnName, line, e.getMessage()));
            }
            return null;
        }
    }

    private static Jcs.Json singleReturnExpression(TreePath methodPath, BlockTree body, Emitter emitter) {
        if (body.getStatements().size() == 1 && body.getStatements().get(0) instanceof ReturnTree ret && ret.getExpression() != null) {
            return emitter.emitExpression(new TreePath(new TreePath(methodPath, body), ret.getExpression()));
        }
        return null;
    }

    private static final class Emitter {
        private final Parsed parsed;
        private final CompilationUnitTree unit;
        private final String sourcePath;
        private final String fnName;
        private final Set<String> locals = new LinkedHashSet<>();
        private final Map<String, Effect> effects = new LinkedHashMap<>();
        private final List<Jcs.Json> panicLoci = new ArrayList<>();

        Emitter(Parsed parsed, CompilationUnitTree unit, String sourcePath, String fnName) {
            this.parsed = parsed;
            this.unit = unit;
            this.sourcePath = sourcePath;
            this.fnName = fnName;
        }

        void addLocal(String name) {
            locals.add(name);
        }

        Jcs.Json emitStatement(TreePath path) {
            Tree tree = path.getLeaf();
            return switch (tree.getKind()) {
                case BLOCK -> emitBlock(path, (BlockTree) tree);
                case RETURN -> {
                    ReturnTree ret = (ReturnTree) tree;
                    yield ctor("java:return", ret.getExpression() == null ? intConst(0) : emitExpression(new TreePath(path, ret.getExpression())));
                }
                case VARIABLE -> emitVariable(path, (VariableTree) tree);
                case EXPRESSION_STATEMENT -> emitExpression(new TreePath(path, ((ExpressionStatementTree) tree).getExpression()));
                case IF -> emitIf(path, (IfTree) tree);
                case WHILE_LOOP -> emitWhile(path, (WhileLoopTree) tree);
                case FOR_LOOP -> emitFor(path, (ForLoopTree) tree);
                case ENHANCED_FOR_LOOP -> emitEnhancedFor(path, (EnhancedForLoopTree) tree);
                case DO_WHILE_LOOP -> emitDo(path, (DoWhileLoopTree) tree);
                case BREAK -> ctor("java:break", intConst(0));
                case CONTINUE -> ctor("java:continue", intConst(0));
                case THROW -> {
                    addEffect(Effect.panics());
                    ThrowTree t = (ThrowTree) tree;
                    TreePath expressionPath = new TreePath(path, t.getExpression());
                    Jcs.Json thrownValue = emitExpression(expressionPath);
                    panicLoci.add(runtimeFailureLocus(
                        path,
                        thrownValue,
                        "explicit-throw",
                        exceptionClass(t.getExpression())
                    ));
                    yield ctor("java:throw", thrownValue);
                }
                case EMPTY_STATEMENT -> skip();
                default -> throw refuse(path, "unhandled statement kind: " + tree.getKind());
            };
        }

        private Jcs.Obj emitBlock(TreePath path, BlockTree block) {
            List<Jcs.Json> terms = new ArrayList<>();
            for (StatementTree stmt : block.getStatements()) {
                terms.add(emitStatement(new TreePath(path, stmt)));
            }
            return seq(terms);
        }

        private Jcs.Json emitVariable(TreePath path, VariableTree variable) {
            String name = variable.getName().toString();
            locals.add(name);
            Jcs.Json init = variable.getInitializer() == null ? intConst(0) : emitExpression(new TreePath(path, variable.getInitializer()));
            return ctor("java:decl", stringConst(name), init);
        }

        private Jcs.Json emitIf(TreePath path, IfTree tree) {
            Jcs.Json cond = emitExpression(new TreePath(path, tree.getCondition()));
            Jcs.Json thenBranch = emitStatement(new TreePath(path, tree.getThenStatement()));
            Jcs.Json elseBranch = tree.getElseStatement() == null ? skip() : emitStatement(new TreePath(path, tree.getElseStatement()));
            return ctor("java:if", cond, thenBranch, elseBranch);
        }

        private Jcs.Json emitWhile(TreePath path, WhileLoopTree tree) {
            Jcs.Obj loop = ctor("java:while", emitExpression(new TreePath(path, tree.getCondition())), emitStatement(new TreePath(path, tree.getStatement())));
            addEffect(Effect.opaqueLoop(Jcs.cid(loop)));
            return loop;
        }

        private Jcs.Json emitDo(TreePath path, DoWhileLoopTree tree) {
            Jcs.Obj loop = ctor("java:do", emitStatement(new TreePath(path, tree.getStatement())), emitExpression(new TreePath(path, tree.getCondition())));
            addEffect(Effect.opaqueLoop(Jcs.cid(loop)));
            return loop;
        }

        private Jcs.Json emitFor(TreePath path, ForLoopTree tree) {
            List<Jcs.Json> initTerms = new ArrayList<>();
            for (StatementTree init : tree.getInitializer()) initTerms.add(emitStatement(new TreePath(path, init)));
            List<Jcs.Json> updateTerms = new ArrayList<>();
            for (ExpressionStatementTree update : tree.getUpdate()) updateTerms.add(emitStatement(new TreePath(path, update)));
            Jcs.Json cond = tree.getCondition() == null ? boolConst(true) : emitExpression(new TreePath(path, tree.getCondition()));
            Jcs.Obj loop = ctor("java:for", seq(initTerms), cond, seq(updateTerms), emitStatement(new TreePath(path, tree.getStatement())));
            addEffect(Effect.opaqueLoop(Jcs.cid(loop)));
            return loop;
        }

        private Jcs.Json emitEnhancedFor(TreePath path, EnhancedForLoopTree tree) {
            if (!tree.getVariable().getModifiers().getAnnotations().isEmpty()) {
                throw refuse(path, "annotated enhanced-for variables are not in the java-source v1 slice");
            }
            String name = tree.getVariable().getName().toString();
            locals.add(name);
            Jcs.Obj loop = ctor("java:foreach", stringConst(name), emitExpression(new TreePath(path, tree.getExpression())), emitStatement(new TreePath(path, tree.getStatement())));
            addEffect(Effect.opaqueLoop(Jcs.cid(loop)));
            return loop;
        }

        Jcs.Json emitExpression(TreePath path) {
            Tree tree = path.getLeaf();
            return switch (tree.getKind()) {
                case INT_LITERAL, LONG_LITERAL, BOOLEAN_LITERAL, STRING_LITERAL, CHAR_LITERAL, NULL_LITERAL -> emitLiteral((LiteralTree) tree);
                case IDENTIFIER -> emitIdentifier(path, (IdentifierTree) tree, true);
                case MEMBER_SELECT -> emitMemberSelect(path, (MemberSelectTree) tree, true);
                case PARENTHESIZED -> emitExpression(new TreePath(path, ((ParenthesizedTree) tree).getExpression()));
                case PLUS, MINUS, MULTIPLY, DIVIDE, REMAINDER,
                    EQUAL_TO, NOT_EQUAL_TO, LESS_THAN, LESS_THAN_EQUAL, GREATER_THAN, GREATER_THAN_EQUAL,
                    CONDITIONAL_AND, CONDITIONAL_OR, AND, OR, XOR, LEFT_SHIFT, RIGHT_SHIFT, UNSIGNED_RIGHT_SHIFT -> emitBinary(path, (BinaryTree) tree);
                case UNARY_MINUS, UNARY_PLUS, LOGICAL_COMPLEMENT, BITWISE_COMPLEMENT, PREFIX_INCREMENT, PREFIX_DECREMENT, POSTFIX_INCREMENT, POSTFIX_DECREMENT -> emitUnary(path, (UnaryTree) tree);
                case ASSIGNMENT -> emitAssignment(path, (AssignmentTree) tree);
                case PLUS_ASSIGNMENT, MINUS_ASSIGNMENT, MULTIPLY_ASSIGNMENT, DIVIDE_ASSIGNMENT, REMAINDER_ASSIGNMENT,
                    AND_ASSIGNMENT, OR_ASSIGNMENT, XOR_ASSIGNMENT, LEFT_SHIFT_ASSIGNMENT, RIGHT_SHIFT_ASSIGNMENT, UNSIGNED_RIGHT_SHIFT_ASSIGNMENT -> emitCompoundAssignment(path, (CompoundAssignmentTree) tree);
                case METHOD_INVOCATION -> emitMethodInvocation(path, (MethodInvocationTree) tree);
                case CONDITIONAL_EXPRESSION -> emitConditional(path, (ConditionalExpressionTree) tree);
                case TYPE_CAST -> emitCast(path, (TypeCastTree) tree);
                case ARRAY_ACCESS -> emitArrayAccess(path, (ArrayAccessTree) tree, true);
                case NEW_CLASS -> emitNewClass(path, (NewClassTree) tree);
                default -> throw refuse(path, "unhandled expression kind: " + tree.getKind());
            };
        }

        private Jcs.Json emitLiteral(LiteralTree lit) {
            Object value = lit.getValue();
            if (value == null) return Jcs.object("kind", Jcs.string("const"), "sort", sort("Ref"), "value", Jcs.nullValue());
            if (value instanceof Boolean b) return boolConst(b);
            if (value instanceof Number n) return intConst(n.longValue());
            if (value instanceof Character c) return intConst(c.charValue());
            return stringConst(value.toString());
        }

        private Jcs.Json emitIdentifier(TreePath path, IdentifierTree id, boolean countRead) {
            Element element = parsed.trees().getElement(path);
            if (countRead && isField(element)) addEffect(Effect.reads(cellName(element, id.getName().toString())));
            return var(id.getName().toString());
        }

        private Jcs.Json emitMemberSelect(TreePath path, MemberSelectTree select, boolean countRead) {
            TreePath exprPath = new TreePath(path, select.getExpression());
            Element element = parsed.trees().getElement(path);
            if (countRead && isField(element)) addEffect(Effect.reads(cellName(element, select.getIdentifier().toString())));
            return ctor("java:member", emitExpression(exprPath), stringConst(select.getIdentifier().toString()));
        }

        private Jcs.Json emitBinary(TreePath path, BinaryTree binary) {
            return ctor(opForBinary(binary.getKind()), emitExpression(new TreePath(path, binary.getLeftOperand())), emitExpression(new TreePath(path, binary.getRightOperand())));
        }

        private Jcs.Json emitUnary(TreePath path, UnaryTree unary) {
            String op = switch (unary.getKind()) {
                case UNARY_MINUS -> "java:neg";
                case UNARY_PLUS -> "java:plus";
                case LOGICAL_COMPLEMENT -> "java:not";
                case BITWISE_COMPLEMENT -> "java:bitnot";
                case PREFIX_INCREMENT -> "java:preinc";
                case PREFIX_DECREMENT -> "java:predec";
                case POSTFIX_INCREMENT -> "java:postinc";
                case POSTFIX_DECREMENT -> "java:postdec";
                default -> throw refuse(path, "unhandled unary kind: " + unary.getKind());
            };
            TreePath exprPath = new TreePath(path, unary.getExpression());
            if (unary.getKind().name().contains("INCREMENT") || unary.getKind().name().contains("DECREMENT")) addWriteIfNonLocal(exprPath);
            return ctor(op, emitExpression(exprPath));
        }

        private Jcs.Json emitAssignment(TreePath path, AssignmentTree assignment) {
            TreePath targetPath = new TreePath(path, assignment.getVariable());
            addWriteIfNonLocal(targetPath);
            return ctor("java:assign", emitTarget(targetPath), emitExpression(new TreePath(path, assignment.getExpression())));
        }

        private Jcs.Json emitCompoundAssignment(TreePath path, CompoundAssignmentTree assignment) {
            TreePath targetPath = new TreePath(path, assignment.getVariable());
            addWriteIfNonLocal(targetPath);
            Jcs.Json readTarget = emitExpression(targetPath);
            Jcs.Json value = emitExpression(new TreePath(path, assignment.getExpression()));
            return ctor("java:assign", emitTarget(targetPath), ctor(opForCompound(assignment.getKind()), readTarget, value));
        }

        private Jcs.Json emitMethodInvocation(TreePath path, MethodInvocationTree invocation) {
            if (!invocation.getTypeArguments().isEmpty()) throw refuse(path, "generic method invocations are not in the java-source v1 slice");
            Element element = parsed.trees().getElement(path);
            String callee = element instanceof ExecutableElement executable
                ? functionName(executable, parsed.types(), parsed.elements())
                : invocation.getMethodSelect().toString();
            if (isIoInvocation(invocation)) addEffect(Effect.io());
            addEffect(Effect.unresolvedCall(callee));
            List<Jcs.Json> args = new ArrayList<>();
            args.add(stringConst(callee));
            for (ExpressionTree arg : invocation.getArguments()) args.add(emitExpression(new TreePath(path, arg)));
            return ctor("java:call", args.toArray(Jcs.Json[]::new));
        }

        private boolean isIoInvocation(MethodInvocationTree invocation) {
            String select = invocation.getMethodSelect().toString();
            return select.startsWith("System.out.") || select.startsWith("System.err.")
                || select.startsWith("java.lang.System.out.") || select.startsWith("java.lang.System.err.");
        }

        private Jcs.Json emitConditional(TreePath path, ConditionalExpressionTree tree) {
            return ctor("java:ite", emitExpression(new TreePath(path, tree.getCondition())), emitExpression(new TreePath(path, tree.getTrueExpression())), emitExpression(new TreePath(path, tree.getFalseExpression())));
        }

        private Jcs.Json emitCast(TreePath path, TypeCastTree tree) {
            return ctor("java:cast", stringConst(tree.getType().toString()), emitExpression(new TreePath(path, tree.getExpression())));
        }

        private Jcs.Json emitArrayAccess(TreePath path, ArrayAccessTree tree, boolean countRead) {
            return ctor("java:index", emitExpression(new TreePath(path, tree.getExpression())), emitExpression(new TreePath(path, tree.getIndex())));
        }

        private Jcs.Json emitNewClass(TreePath path, NewClassTree tree) {
            if (tree.getClassBody() != null) throw refuse(path, "anonymous classes are not in the java-source v1 slice");
            List<Jcs.Json> args = new ArrayList<>();
            args.add(stringConst(tree.getIdentifier().toString()));
            for (ExpressionTree arg : tree.getArguments()) args.add(emitExpression(new TreePath(path, arg)));
            return ctor("java:new", args.toArray(Jcs.Json[]::new));
        }

        private Jcs.Json emitTarget(TreePath path) {
            Tree tree = path.getLeaf();
            return switch (tree.getKind()) {
                case IDENTIFIER -> emitIdentifier(path, (IdentifierTree) tree, false);
                case MEMBER_SELECT -> emitMemberSelect(path, (MemberSelectTree) tree, false);
                case ARRAY_ACCESS -> emitArrayAccess(path, (ArrayAccessTree) tree, false);
                default -> throw refuse(path, "unsupported assignment target kind: " + tree.getKind());
            };
        }

        private void addWriteIfNonLocal(TreePath targetPath) {
            Tree tree = targetPath.getLeaf();
            Element element = parsed.trees().getElement(targetPath);
            if (isField(element)) {
                addEffect(Effect.writes(cellName(element, tree.toString())));
                return;
            }
            if (tree instanceof ArrayAccessTree access) {
                ExpressionTree expr = access.getExpression();
                if (!(expr instanceof IdentifierTree id) || !locals.contains(id.getName().toString())) {
                    addEffect(Effect.writes("array:" + expr));
                }
            }
        }

        private List<Jcs.Json> effectsJson() {
            return effects.values().stream()
                .sorted(Comparator.comparing(Effect::sortKey))
                .map(e -> (Jcs.Json) e.toJson())
                .toList();
        }

        private List<Jcs.Json> panicLociJson() {
            return List.copyOf(panicLoci);
        }

        private void addEffect(Effect effect) {
            effects.putIfAbsent(effect.sortKey(), effect);
        }

        private Jcs.Obj runtimeFailureLocus(TreePath path, Jcs.Json argTerm, String subkind, String exceptionClass) {
            List<Object> fields = new ArrayList<>(List.of(
                "effectKind", Jcs.string(PANIC_FREEDOM_EFFECT_KIND),
                "callee", Jcs.string(RUNTIME_FAILURE_SITE_CONCEPT),
                "subkind", Jcs.string(subkind),
                "argTerm", argTerm,
                "file", Jcs.string(sourcePath),
                "line", Jcs.integer(lineOf(parsed.trees(), unit, path.getLeaf())),
                "col", Jcs.integer(columnOf(parsed.trees(), unit, path.getLeaf()))
            ));
            if (exceptionClass != null && !exceptionClass.isEmpty()) {
                fields.add("exceptionClass");
                fields.add(Jcs.string(exceptionClass));
            }
            return Jcs.object(fields.toArray());
        }

        private String exceptionClass(ExpressionTree expression) {
            if (expression instanceof NewClassTree created) {
                return created.getIdentifier().toString();
            }
            return null;
        }

        private RefuseException refuse(TreePath path, String reason) {
            return new RefuseException(path.getLeaf().getKind().name(), lineOf(parsed.trees(), unit, path.getLeaf()), reason);
        }
    }

    private record Effect(String kind, String key, String value) {
        static Effect reads(String target) { return new Effect("reads", "target", target); }
        static Effect writes(String target) { return new Effect("writes", "target", target); }
        static Effect io() { return new Effect("io", null, null); }
        static Effect panics() { return new Effect("panics", null, null); }
        static Effect unresolvedCall(String name) { return new Effect("unresolved_call", "name", name); }
        static Effect opaqueLoop(String cid) { return new Effect("opaque_loop", "loopCid", cid); }

        String sortKey() {
            return switch (kind) {
                case "reads" -> "0:reads:" + value;
                case "writes" -> "1:writes:" + value;
                case "io" -> "2:io";
                case "unsafe" -> "3:unsafe";
                case "panics" -> "4:panics";
                case "unresolved_call" -> "5:unresolved_call:" + value;
                case "opaque_loop" -> "6:opaque_loop:" + value;
                default -> "99:" + kind + ":" + value;
            };
        }

        Jcs.Obj toJson() {
            if (key == null) return Jcs.object("kind", Jcs.string(kind));
            return Jcs.object("kind", Jcs.string(kind), key, Jcs.string(value));
        }
    }

    private static final class RefuseException extends RuntimeException {
        final String kind;
        final int line;

        RefuseException(String kind, int line, String message) {
            super(message);
            this.kind = kind;
            this.line = line;
        }
    }

    private static boolean isField(Element element) {
        return element != null && (element.getKind() == ElementKind.FIELD || element.getKind() == ElementKind.ENUM_CONSTANT);
    }

    private static String cellName(Element element, String fallback) {
        if (element == null) return fallback;
        Element owner = element.getEnclosingElement();
        if (owner instanceof TypeElement type) {
            return type.getQualifiedName() + "." + element.getSimpleName();
        }
        return element.getSimpleName().toString();
    }

    private static String functionName(ExecutableElement executable, Types types, Elements elements) {
        Element owner = executable.getEnclosingElement();
        String ownerName = owner instanceof TypeElement type ? elements.getBinaryName(type).toString() : owner.toString();
        String params = executable.getParameters().stream()
            .map(p -> erasedName(types.erasure(p.asType())))
            .reduce((a, b) -> a + "," + b)
            .orElse("");
        return ownerName + "." + executable.getSimpleName() + "(" + params + ")";
    }

    private static String erasedName(TypeMirror type) {
        return type.toString();
    }

    private static Jcs.Obj sortFor(TypeMirror type) {
        return switch (type.getKind()) {
            case BOOLEAN -> sort("Bool");
            case BYTE, SHORT, INT, LONG, CHAR -> sort("Int");
            case FLOAT, DOUBLE -> sort("Real");
            case VOID -> sort("Unit");
            default -> "java.lang.String".equals(type.toString()) ? sort("String") : sort("Ref");
        };
    }

    private static String opForBinary(Tree.Kind kind) {
        return switch (kind) {
            case PLUS -> "java:add";
            case MINUS -> "java:sub";
            case MULTIPLY -> "java:mul";
            case DIVIDE -> "java:div";
            case REMAINDER -> "java:mod";
            case EQUAL_TO -> "java:eq";
            case NOT_EQUAL_TO -> "java:ne";
            case LESS_THAN -> "java:lt";
            case LESS_THAN_EQUAL -> "java:le";
            case GREATER_THAN -> "java:gt";
            case GREATER_THAN_EQUAL -> "java:ge";
            case CONDITIONAL_AND -> "java:and";
            case CONDITIONAL_OR -> "java:or";
            case AND -> "java:bitand";
            case OR -> "java:bitor";
            case XOR -> "java:bitxor";
            case LEFT_SHIFT -> "java:shl";
            case RIGHT_SHIFT -> "java:shr";
            case UNSIGNED_RIGHT_SHIFT -> "java:ushr";
            default -> throw new IllegalArgumentException("unhandled binary kind " + kind);
        };
    }

    private static String opForCompound(Tree.Kind kind) {
        return switch (kind) {
            case PLUS_ASSIGNMENT -> "java:add";
            case MINUS_ASSIGNMENT -> "java:sub";
            case MULTIPLY_ASSIGNMENT -> "java:mul";
            case DIVIDE_ASSIGNMENT -> "java:div";
            case REMAINDER_ASSIGNMENT -> "java:mod";
            case AND_ASSIGNMENT -> "java:bitand";
            case OR_ASSIGNMENT -> "java:bitor";
            case XOR_ASSIGNMENT -> "java:bitxor";
            case LEFT_SHIFT_ASSIGNMENT -> "java:shl";
            case RIGHT_SHIFT_ASSIGNMENT -> "java:shr";
            case UNSIGNED_RIGHT_SHIFT_ASSIGNMENT -> "java:ushr";
            default -> throw new IllegalArgumentException("unhandled compound kind " + kind);
        };
    }

    private static int lineOf(Trees trees, CompilationUnitTree unit, Tree tree) {
        SourcePositions positions = trees.getSourcePositions();
        long pos = positions.getStartPosition(unit, tree);
        if (pos < 0 || unit.getLineMap() == null) return 0;
        return (int) unit.getLineMap().getLineNumber(pos);
    }

    private static int columnOf(Trees trees, CompilationUnitTree unit, Tree tree) {
        SourcePositions positions = trees.getSourcePositions();
        long pos = positions.getStartPosition(unit, tree);
        if (pos < 0 || unit.getLineMap() == null) return 0;
        return (int) unit.getLineMap().getColumnNumber(pos);
    }

    private static Jcs.Obj wrapSeqFromContracts(List<Jcs.Json> decls, int start) {
        List<Jcs.Json> terms = new ArrayList<>();
        for (int i = start; i < decls.size(); i++) {
            if (decls.get(i) instanceof Jcs.Obj obj && "function-contract".equals(obj.stringFieldOrNull("kind"))) {
                Jcs.Json post = obj.get("post");
                if (post instanceof Jcs.Obj postObj) {
                    Jcs.Arr args = postObj.arrayField("args");
                    if (args.values().size() > 1) terms.add(args.get(1));
                }
            }
        }
        return seq(terms);
    }

    private static Jcs.Obj seq(List<Jcs.Json> terms) {
        if (terms.isEmpty()) return skip();
        Jcs.Json result = terms.get(0);
        for (int i = 1; i < terms.size(); i++) result = ctor("java:seq", result, terms.get(i));
        return (Jcs.Obj) result;
    }

    private static Jcs.Obj functionContract(
        String fnName,
        List<String> formals,
        List<Jcs.Json> formalSorts,
        Jcs.Obj returnSort,
        Jcs.Obj post,
        List<Jcs.Json> effects,
        List<Jcs.Json> panicLoci,
        String file,
        int line,
        int col
    ) {
        List<Object> fields = new ArrayList<>(List.of(
            "schemaVersion", Jcs.string("1"),
            "kind", Jcs.string("function-contract"),
            "fnName", Jcs.string(fnName),
            "formals", Jcs.array(formals.stream().map(Jcs::string).toList()),
            "formalSorts", Jcs.array(formalSorts),
            "returnSort", returnSort,
            "pre", trueFormula(),
            "post", post,
            "bodyCid", Jcs.nullValue(),
            "effects", Jcs.array(effects),
            "locus", Jcs.object("file", Jcs.string(file), "line", Jcs.integer(line), "col", Jcs.integer(col)),
            "autoMintedMementos", Jcs.array()
        ));
        if (!panicLoci.isEmpty()) {
            fields.add("panicLoci");
            fields.add(Jcs.array(panicLoci));
        }
        return Jcs.object(fields.toArray());
    }

    private static Jcs.Obj trueFormula() {
        return Jcs.object("kind", Jcs.string("atomic"), "name", Jcs.string("true"), "args", Jcs.array());
    }

    private static Jcs.Obj eq(Jcs.Json lhs, Jcs.Json rhs) {
        return Jcs.object("kind", Jcs.string("atomic"), "name", Jcs.string("="), "args", Jcs.array(lhs, rhs));
    }

    private static Jcs.Obj ctor(String name, Jcs.Json... args) {
        return Jcs.object("kind", Jcs.string("ctor"), "name", Jcs.string(name), "args", Jcs.array(args));
    }

    private static Jcs.Obj var(String name) {
        return Jcs.object("kind", Jcs.string("var"), "name", Jcs.string(name));
    }

    private static Jcs.Obj intConst(long value) {
        return Jcs.object("kind", Jcs.string("const"), "sort", sort("Int"), "value", Jcs.integer(value));
    }

    private static Jcs.Obj boolConst(boolean value) {
        return Jcs.object("kind", Jcs.string("const"), "sort", sort("Bool"), "value", Jcs.bool(value));
    }

    private static Jcs.Obj stringConst(String value) {
        return Jcs.object("kind", Jcs.string("const"), "sort", sort("String"), "value", Jcs.string(value));
    }

    private static Jcs.Obj sort(String name) {
        return Jcs.object("kind", Jcs.string("primitive"), "name", Jcs.string(name));
    }

    private static Jcs.Obj skip() {
        return ctor("java:skip", intConst(0));
    }

    private static Jcs.Obj diag(String severity, String message) {
        return Jcs.object("severity", Jcs.string(severity), "message", Jcs.string(message));
    }
}
