package com.provekit.lift.termshape;

import com.github.javaparser.ast.Node;
import com.github.javaparser.ast.body.MethodDeclaration;
import com.github.javaparser.ast.body.Parameter;
import com.github.javaparser.ast.expr.*;
import com.github.javaparser.ast.stmt.*;
import com.github.javaparser.ast.comments.Comment;
import com.provekit.ir.Jcs;
import com.provekit.ir.Jcs.Json;

import java.util.ArrayList;
import java.util.List;
import java.util.Optional;

/**
 * Walks a java MethodDeclaration and produces a ProofIR term_shape for
 * its body. Substrate-honest:
 *
 * <ul>
 *   <li>Citation comments ({@code @concept X}) on an AST node take
 *       precedence — the comment's concept identity wins.</li>
 *   <li>Native java patterns the lifter knows are recognized
 *       structurally.</li>
 *   <li>Every other AST node records a {@code loss_record} entry
 *       naming the unrecognized node class + source text + location.
 *       That entry IS the work item: implement a recognizer for that
 *       pattern, the loss entry retires.</li>
 * </ul>
 */
public final class TermShapeLifter {

    public record LiftedMethod(
            Jcs.Json termShape,
            List<Jcs.Json> paramNames,
            List<Jcs.Json> paramTypes,
            String returnType,
            List<Jcs.Json> lossRecords) {}

    public LiftedMethod liftMethod(MethodDeclaration method) {
        List<Json> paramNames = new ArrayList<>();
        List<Json> paramTypes = new ArrayList<>();
        for (Parameter p : method.getParameters()) {
            paramNames.add(Jcs.string(p.getNameAsString()));
            paramTypes.add(Jcs.string(p.getType().asString()));
        }
        String returnType = method.getType().asString();

        List<Json> losses = new ArrayList<>();
        Json shape = method.getBody()
                .map(body -> liftBlock(body, losses))
                .orElseGet(Jcs::object);

        // Tail-expression form: when the function body is a single
        // concept:return wrapping an expression, the rust source had
        // a tail expression (no `return` keyword). Strip the wrapper
        // for substrate-symmetric cycle closure.
        //
        // ALSO applies when the body is concept:seq[..., concept:return(X)]
        // with X being the final value-producing expression — strip the
        // return on the LAST element only.
        shape = stripTrailingReturn(shape);

        return new LiftedMethod(shape, paramNames, paramTypes, returnType, losses);
    }

    /** Strip outer/trailing concept:return wrapper for substrate-symmetric
     *  closure. Rust source `fn f() -> T { tail_expr }` lifts as just the
     *  tail expression; java emit `return X;` lifts as concept:return(X);
     *  this restores the tail-expression form so the cycle round-trips. */
    private Json stripTrailingReturn(Json shape) {
        if (!(shape instanceof Jcs.Obj obj)) return shape;
        String cn = null;
        Jcs.Json args = null;
        for (Jcs.Field f : obj.fields()) {
            if ("concept_name".equals(f.key()) && f.value() instanceof Jcs.Str s) {
                cn = s.value();
            } else if ("args".equals(f.key())) {
                args = f.value();
            }
        }
        if ("concept:return".equals(cn) && args instanceof Jcs.Arr arr
                && arr.values().size() == 1) {
            return arr.values().get(0);
        }
        if ("concept:seq".equals(cn) && args instanceof Jcs.Arr arr
                && !arr.values().isEmpty()) {
            List<Jcs.Json> children = new ArrayList<>(arr.values());
            int lastIdx = children.size() - 1;
            Jcs.Json last = children.get(lastIdx);
            Jcs.Json strippedLast = stripTrailingReturn(last);
            if (!strippedLast.equals(last)) {
                children.set(lastIdx, strippedLast);
                return Jcs.object(
                    "args", new Jcs.Arr(children),
                    "concept_name", Jcs.string("concept:seq")
                );
            }
        }
        return shape;
    }

    /** Lift a block as concept:seq of its statements. */
    private Json liftBlock(BlockStmt block, List<Json> losses) {
        // Substrate-symmetric match recognition: the rust lower emits
        // `match scrut { pat1 => body1, _ => body2 }` as java
        // `var __provekit_vN = scrut; if (pat-as-cond) { body1 } else { body2 }`.
        // Detect this canonical 2-statement pattern and emit concept:match.
        Optional<Json> matchRecognized = tryRecognizeMatch(block, losses);
        if (matchRecognized.isPresent()) return matchRecognized.get();
        // Struct destructure: `var __provekit_struct = expr; var a = __provekit_struct.get("a"); var b = __provekit_struct.get("b");`
        // → concept:destructure-struct(expr, type_leaf, a, b)
        List<Json> structSweep = tryRecognizeStructDestructure(block, losses);
        if (structSweep != null) {
            if (structSweep.size() == 1) return structSweep.get(0);
            return Jcs.object(
                "args", new Jcs.Arr(structSweep),
                "concept_name", Jcs.string("concept:seq")
            );
        }
        // Tuple destructure recognition: rust `let (a, b) = expr;`
        // emits `Object[] __tuple = expr; var a = __tuple[0]; var b = __tuple[1];`.
        // Detect this 3-statement (or N+1) pattern and emit
        // concept:destructure-tuple(value, name1, name2, ...).
        List<Json> destructure = tryRecognizeTupleDestructure(block, losses);
        if (destructure != null) {
            if (destructure.size() == 1) return destructure.get(0);
            return Jcs.object(
                "args", new Jcs.Arr(destructure),
                "concept_name", Jcs.string("concept:seq")
            );
        }
        // While-let recognition: rust `while let Some(x) = expr { body }`
        // emits `var x = expr; while (x != null) { body... x = expr; }`.
        // Walk the block looking for this two-stmt run and emit the
        // remaining stmts AROUND it as a flatter seq.
        List<Json> sweep = tryRecognizeWhileLet(block, losses);
        if (sweep != null) {
            if (sweep.size() == 1) return sweep.get(0);
            return Jcs.object(
                "args", new Jcs.Arr(sweep),
                "concept_name", Jcs.string("concept:seq")
            );
        }
        List<Json> stmts = new ArrayList<>();
        for (Statement s : block.getStatements()) {
            Json lifted = liftStatement(s, losses);
            if (lifted != null) stmts.add(lifted);
        }
        if (stmts.size() == 1) return stmts.get(0);
        return Jcs.object(
            "args", new Jcs.Arr(stmts),
            "concept_name", Jcs.string("concept:seq")
        );
    }

    /** Scan a block for rust's `let TypeName { a, b } = expr;` emission:
     *  `var __provekit_struct = expr; var a = __provekit_struct.get("a"); ...`
     *  Returns concept:destructure-struct alongside surrounding statements. */
    private List<Json> tryRecognizeStructDestructure(BlockStmt block, List<Json> losses) {
        List<Statement> stmts = block.getStatements();
        for (int i = 0; i < stmts.size() - 1; i++) {
            Statement first = stmts.get(i);
            String structVar = null;
            com.github.javaparser.ast.expr.Expression initExpr = null;
            if (first instanceof com.github.javaparser.ast.stmt.ExpressionStmt es
                    && es.getExpression() instanceof com.github.javaparser.ast.expr.VariableDeclarationExpr vde
                    && vde.getVariables().size() == 1) {
                var v0 = vde.getVariable(0);
                if (v0.getInitializer().isPresent()
                        && v0.getNameAsString().startsWith("__provekit_struct")) {
                    structVar = v0.getNameAsString();
                    initExpr = v0.getInitializer().get();
                }
            }
            if (structVar == null) continue;
            // Collect `var X = __provekit_struct.get("X");` lines.
            List<String[]> fields = new ArrayList<>(); // [binding, field-name]
            int j = i + 1;
            while (j < stmts.size()) {
                Statement s = stmts.get(j);
                if (!(s instanceof com.github.javaparser.ast.stmt.ExpressionStmt sesxs)) break;
                if (!(sesxs.getExpression() instanceof com.github.javaparser.ast.expr.VariableDeclarationExpr svde)) break;
                if (svde.getVariables().size() != 1) break;
                var sv = svde.getVariable(0);
                if (sv.getInitializer().isEmpty()) break;
                com.github.javaparser.ast.expr.Expression vinit = sv.getInitializer().get();
                if (!(vinit instanceof MethodCallExpr getCall)) break;
                if (!"get".equals(getCall.getNameAsString())) break;
                if (getCall.getScope().isEmpty()
                        || !getCall.getScope().get().toString().equals(structVar)) break;
                if (getCall.getArguments().size() != 1) break;
                String fieldKey = getCall.getArgument(0).toString();
                if (fieldKey.startsWith("\"") && fieldKey.endsWith("\"")) {
                    fieldKey = fieldKey.substring(1, fieldKey.length() - 1);
                }
                fields.add(new String[]{sv.getNameAsString(), fieldKey});
                j++;
            }
            if (fields.isEmpty()) continue;
            List<Json> out = new ArrayList<>();
            for (int k = 0; k < i; k++) {
                Json s = liftStatement(stmts.get(k), losses);
                if (s != null) out.add(s);
            }
            List<Json> destructArgs = new ArrayList<>();
            destructArgs.add(liftExpression(initExpr, losses));
            destructArgs.add(Jcs.object("kind", Jcs.string("type"), "text", Jcs.string("LiftResult")));
            for (String[] f : fields) {
                destructArgs.add(Jcs.object(
                    "kind", Jcs.string("symbol"),
                    "text", Jcs.string(f[0]),
                    "field_name", Jcs.string(f[1])
                ));
            }
            out.add(Jcs.object(
                "args", new Jcs.Arr(destructArgs),
                "concept_name", Jcs.string("concept:destructure-struct")
            ));
            for (int k = j; k < stmts.size(); k++) {
                Json s = liftStatement(stmts.get(k), losses);
                if (s != null) out.add(s);
            }
            return out;
        }
        return null;
    }

    /** Scan a block for the rust `let (a, b, ...) = expr` emission:
     *  `Object[] __tuple = expr; var a = __tuple[0]; var b = __tuple[1]; ...`
     *  Returns the list of lifted statements with the tuple expansion
     *  collapsed to a single concept:destructure-tuple. */
    private List<Json> tryRecognizeTupleDestructure(BlockStmt block, List<Json> losses) {
        List<Statement> stmts = block.getStatements();
        for (int i = 0; i < stmts.size() - 1; i++) {
            Statement first = stmts.get(i);
            String tupleVar = null;
            com.github.javaparser.ast.expr.Expression initExpr = null;
            if (first instanceof com.github.javaparser.ast.stmt.ExpressionStmt es
                    && es.getExpression() instanceof com.github.javaparser.ast.expr.VariableDeclarationExpr vde
                    && vde.getVariables().size() == 1) {
                var v0 = vde.getVariable(0);
                if (v0.getInitializer().isPresent()
                        && v0.getNameAsString().startsWith("__provekit_tuple")) {
                    tupleVar = v0.getNameAsString();
                    initExpr = v0.getInitializer().get();
                }
            }
            if (tupleVar == null) continue;
            // Collect consecutive `var X = __tuple[N];` statements.
            List<String> names = new ArrayList<>();
            int j = i + 1;
            while (j < stmts.size()) {
                Statement s = stmts.get(j);
                if (!(s instanceof com.github.javaparser.ast.stmt.ExpressionStmt sesxs)) break;
                if (!(sesxs.getExpression() instanceof com.github.javaparser.ast.expr.VariableDeclarationExpr svde)) break;
                if (svde.getVariables().size() != 1) break;
                var sv = svde.getVariable(0);
                if (sv.getInitializer().isEmpty()) break;
                com.github.javaparser.ast.expr.Expression vinit = sv.getInitializer().get();
                if (!(vinit instanceof com.github.javaparser.ast.expr.ArrayAccessExpr aae)) break;
                if (!aae.getName().toString().equals(tupleVar)) break;
                names.add(sv.getNameAsString());
                j++;
            }
            if (names.isEmpty()) continue;
            // Build the result: pre-stmts + destructure + post-stmts.
            List<Json> out = new ArrayList<>();
            for (int k = 0; k < i; k++) {
                Json s = liftStatement(stmts.get(k), losses);
                if (s != null) out.add(s);
            }
            List<Json> destructArgs = new ArrayList<>();
            destructArgs.add(liftExpression(initExpr, losses));
            for (String n : names) {
                destructArgs.add(Jcs.object(
                    "kind", Jcs.string("symbol"),
                    "text", Jcs.string(n)
                ));
            }
            out.add(Jcs.object(
                "args", new Jcs.Arr(destructArgs),
                "concept_name", Jcs.string("concept:destructure-tuple")
            ));
            for (int k = j; k < stmts.size(); k++) {
                Json s = liftStatement(stmts.get(k), losses);
                if (s != null) out.add(s);
            }
            return out;
        }
        return null;
    }

    /** Scan a block for the rust `while let Some(x) = expr { body }`
     *  emission pattern: `var x = expr; while (x != null) { body...
     *  x = expr; }`. When found, lifts as concept:while-let alongside
     *  the surrounding statements. Returns null if no match. */
    private List<Json> tryRecognizeWhileLet(BlockStmt block, List<Json> losses) {
        List<Statement> stmts = block.getStatements();
        // Look for pairs (init-var, while-loop) anywhere in the block.
        for (int i = 0; i < stmts.size() - 1; i++) {
            Statement first = stmts.get(i);
            Statement second = stmts.get(i + 1);
            // first: ExpressionStmt VariableDeclarationExpr with init
            String varName = null;
            com.github.javaparser.ast.expr.Expression initExpr = null;
            if (first instanceof com.github.javaparser.ast.stmt.ExpressionStmt es
                    && es.getExpression() instanceof com.github.javaparser.ast.expr.VariableDeclarationExpr vde
                    && vde.getVariables().size() == 1) {
                var v0 = vde.getVariable(0);
                if (v0.getInitializer().isPresent()) {
                    varName = v0.getNameAsString();
                    initExpr = v0.getInitializer().get();
                }
            }
            if (varName == null || initExpr == null) continue;
            // second: while (varName != null) { body... varName = initExpr; }
            if (!(second instanceof com.github.javaparser.ast.stmt.WhileStmt ws)) continue;
            String cond = ws.getCondition().toString().replaceAll("\\s+", "");
            if (!cond.contains(varName + "!=null")) continue;
            if (!(ws.getBody() instanceof BlockStmt loopBody)) continue;
            // Last statement should be `varName = initExpr` (re-assign).
            List<Statement> loopStmts = loopBody.getStatements();
            if (loopStmts.isEmpty()) continue;
            Statement last = loopStmts.get(loopStmts.size() - 1);
            boolean lastReassigns = false;
            if (last instanceof com.github.javaparser.ast.stmt.ExpressionStmt lastEs
                    && lastEs.getExpression() instanceof com.github.javaparser.ast.expr.AssignExpr ae) {
                if (ae.getTarget().toString().equals(varName)
                        && ae.getValue().toString().equals(initExpr.toString())) {
                    lastReassigns = true;
                }
            }
            if (!lastReassigns) continue;
            // Build the rest of the block: pre-stmts (before i) +
            // concept:while-let + post-stmts (after i+1).
            List<Json> out = new ArrayList<>();
            for (int j = 0; j < i; j++) {
                Json s = liftStatement(stmts.get(j), losses);
                if (s != null) out.add(s);
            }
            // Reconstruct concept:while-let. Pattern is "Some(varName)"
            // (the rust source's typical pattern).
            Json patternLeaf = Jcs.object(
                "kind", Jcs.string("symbol"),
                "text", Jcs.string("Some(" + varName + ")")
            );
            Json initShape = liftExpression(initExpr, losses);
            // Body is loop's stmts minus the last (re-assign).
            // Build a synthetic BlockStmt + run liftBlock to get
            // multi-stmt pattern recognition (tuple destructure, etc.).
            com.github.javaparser.ast.NodeList<com.github.javaparser.ast.stmt.Statement> bodyNodes =
                new com.github.javaparser.ast.NodeList<>();
            for (int j = 0; j < loopStmts.size() - 1; j++) {
                bodyNodes.add(loopStmts.get(j));
            }
            com.github.javaparser.ast.stmt.BlockStmt syntheticBody =
                new com.github.javaparser.ast.stmt.BlockStmt(bodyNodes);
            Json bodyShape = liftBlock(syntheticBody, losses);
            out.add(Jcs.object(
                "args", new Jcs.Arr(List.of(patternLeaf, initShape, bodyShape)),
                "concept_name", Jcs.string("concept:while-let")
            ));
            for (int j = i + 2; j < stmts.size(); j++) {
                Json s = liftStatement(stmts.get(j), losses);
                if (s != null) out.add(s);
            }
            return out;
        }
        return null;
    }

    /** Recognize `var __provekit_vN = scrut; if (cond) { then } else { else }`
     *  as concept:match(scrut, arm1, arm2). The cond is mapped back to a
     *  pattern-string heuristically (`!= null && !is_null()` → `Some(v) if
     *  !v.is_null()`); for the catch-all else arm the pattern is `_`. */
    private Optional<Json> tryRecognizeMatch(BlockStmt block, List<Json> losses) {
        List<Statement> stmts = block.getStatements();
        if (stmts.size() != 2) return Optional.empty();
        if (!(stmts.get(0) instanceof com.github.javaparser.ast.stmt.ExpressionStmt es)) return Optional.empty();
        if (!(es.getExpression() instanceof com.github.javaparser.ast.expr.VariableDeclarationExpr vde)) return Optional.empty();
        if (vde.getVariables().size() != 1) return Optional.empty();
        var decl = vde.getVariable(0);
        String binding = decl.getNameAsString();
        if (!binding.startsWith("__provekit_v")) return Optional.empty();
        if (decl.getInitializer().isEmpty()) return Optional.empty();
        com.github.javaparser.ast.expr.Expression scrutExpr = decl.getInitializer().get();
        if (!(stmts.get(1) instanceof com.github.javaparser.ast.stmt.IfStmt ifs)) return Optional.empty();
        if (ifs.getElseStmt().isEmpty()) return Optional.empty();
        Statement thenStmt = ifs.getThenStmt();
        Statement elseStmt = ifs.getElseStmt().get();
        // Build concept:match(scrut, arm1, arm2). For the arm patterns
        // use a heuristic mapping of the condition's shape.
        String arm1Pattern = derivePatternFromCondition(ifs.getCondition(), binding);
        Json scrutShape = liftExpression(scrutExpr, losses);
        Json arm1Body = thenStmt instanceof BlockStmt tb ? liftBlock(tb, losses) : liftStatement(thenStmt, losses);
        Json arm2Body = elseStmt instanceof BlockStmt eb ? liftBlock(eb, losses) : liftStatement(elseStmt, losses);
        // Substitute the synthetic binding (__provekit_vN) with the
        // pattern's named binding (e.g. `v` from `Some(v)`).
        String patternBinding = extractBindingFromPattern(arm1Pattern);
        if (patternBinding != null && !patternBinding.equals(binding)) {
            arm1Body = substituteSymbolBinding(arm1Body, binding, patternBinding);
        }
        // Match arms in rust are expressions: `pat => expr,`. When the
        // arm body is concept:return wrapping a single expression, unwrap
        // it (rust source's arm form is bare expression).
        arm1Body = stripTrailingReturn(arm1Body);
        arm2Body = stripTrailingReturn(arm2Body);
        Json arm1 = Jcs.object(
            "args", new Jcs.Arr(List.of(
                Jcs.object("kind", Jcs.string("symbol"), "text", Jcs.string(arm1Pattern)),
                arm1Body
            )),
            "concept_name", Jcs.string("concept:match-arm")
        );
        Json arm2 = Jcs.object(
            "args", new Jcs.Arr(List.of(
                Jcs.object("kind", Jcs.string("symbol"), "text", Jcs.string("_")),
                arm2Body
            )),
            "concept_name", Jcs.string("concept:match-arm")
        );
        return Optional.of(Jcs.object(
            "args", new Jcs.Arr(List.of(scrutShape, arm1, arm2)),
            "concept_name", Jcs.string("concept:match")
        ));
    }

    /** True if a lifted shape is effectively empty (nothing, concept:skip,
     *  or a concept:seq with no operative children). Used to detect
     *  empty else-branches that should fall back to concept:skip. */
    private boolean isEffectivelyEmpty(Json shape) {
        if (shape == null) return true;
        if (!(shape instanceof Jcs.Obj obj)) return false;
        String cn = null;
        Jcs.Json args = null;
        for (Jcs.Field f : obj.fields()) {
            if ("concept_name".equals(f.key()) && f.value() instanceof Jcs.Str s) cn = s.value();
            else if ("args".equals(f.key())) args = f.value();
        }
        if ("concept:skip".equals(cn)) return true;
        if ("concept:seq".equals(cn) && args instanceof Jcs.Arr arr && arr.values().isEmpty()) return true;
        // Empty object {} from non-operation_shape.
        return obj.fields().isEmpty();
    }

    private Json skipShape() {
        return Jcs.object(
            "args", new Jcs.Arr(List.of()),
            "concept_name", Jcs.string("concept:skip")
        );
    }

    /** Extract the bound variable name from a pattern string like
     *  "Some(v)", "Err(e)", "Some(v) if !v.is_null()". Returns null
     *  for non-binding patterns like "_". */
    private String extractBindingFromPattern(String pattern) {
        if (pattern == null) return null;
        int lparen = pattern.indexOf('(');
        if (lparen < 0) return null;
        int rparen = pattern.indexOf(')', lparen);
        if (rparen < 0) return null;
        String inner = pattern.substring(lparen + 1, rparen).trim();
        if (inner.isEmpty() || "_".equals(inner)) return null;
        // For nested patterns like "Type::Variant(x)" return the deepest binding.
        int innerParen = inner.indexOf('(');
        if (innerParen > 0) {
            return extractBindingFromPattern(inner);
        }
        return inner;
    }

    /** Recursively substitute symbol-leaf text from one binding to another
     *  in a lifted term-shape tree. */
    private Json substituteSymbolBinding(Json node, String oldName, String newName) {
        if (!(node instanceof Jcs.Obj obj)) return node;
        List<Jcs.Field> newFields = new ArrayList<>();
        for (Jcs.Field f : obj.fields()) {
            Json v = f.value();
            if (v instanceof Jcs.Arr arr) {
                List<Jcs.Json> newArr = new ArrayList<>();
                for (Jcs.Json e : arr.values()) {
                    newArr.add(substituteSymbolBinding(e, oldName, newName));
                }
                newFields.add(new Jcs.Field(f.key(), new Jcs.Arr(newArr)));
            } else if (v instanceof Jcs.Obj) {
                newFields.add(new Jcs.Field(f.key(), substituteSymbolBinding(v, oldName, newName)));
            } else if (v instanceof Jcs.Str s) {
                // Rewrite text fields containing the symbol's name.
                if ("text".equals(f.key()) && oldName.equals(s.value())) {
                    newFields.add(new Jcs.Field(f.key(), Jcs.string(newName)));
                } else {
                    newFields.add(f);
                }
            } else {
                newFields.add(f);
            }
        }
        return new Jcs.Obj(newFields);
    }

    /** Heuristic pattern-from-condition mapping. Recognizes the common
     *  java cond forms the lower emits: `X != null` → `Some(v)`,
     *  `X != null && !X.isNull()` → `Some(v) if !v.is_null()`. Falls
     *  back to a synthetic Pattern(binding) for unknown forms. */
    private String derivePatternFromCondition(com.github.javaparser.ast.expr.Expression cond, String binding) {
        // Normalize: remove all whitespace for substring checks since
        // JavaParser may render `v . is_null ()` with spaces from
        // token-stream lifts.
        String t = cond.toString().replaceAll("\\s+", "");
        // Pattern: __provekit_vN != null && !v.is_null() (or .isNull())
        if (t.contains("!=null") && (t.contains(".isNull()") || t.contains(".is_null()"))) {
            return "Some(v) if !v.is_null()";
        }
        if (t.contains("!=null") || t.matches(".*\\binstanceof.*\\.Ok\\b.*")) {
            return "Some(v)";
        }
        if (t.matches(".*\\binstanceof.*\\.Err\\b.*")) {
            return "Err(e)";
        }
        return "Some(v)";
    }

    private Json liftStatement(Statement stmt, List<Json> losses) {
        // Citation-comment short-circuit: if the statement has a leading
        // /*@concept X*/ comment, prefer the citation's concept identity.
        Optional<String> cited = readCitation(stmt);
        if (cited.isPresent()) {
            return reconstructFromCitation(cited.get(), stmt, losses);
        }
        if (stmt instanceof WhileStmt ws) {
            Json cond = liftExpression(ws.getCondition(), losses);
            Json body = ws.getBody() instanceof BlockStmt bb ? liftBlock(bb, losses) : liftStatement(ws.getBody(), losses);
            return Jcs.object(
                "args", new Jcs.Arr(List.of(cond, body)),
                "concept_name", Jcs.string("concept:while")
            );
        }
        if (stmt instanceof IfStmt ifs) {
            Json cond = liftExpression(ifs.getCondition(), losses);
            Json thenBranch = ifs.getThenStmt() instanceof BlockStmt tb ? liftBlock(tb, losses) : liftStatement(ifs.getThenStmt(), losses);
            // Detect "empty else" (substrate's emit for missing source
            // `else` is `else { ; }` or `else {}`); treat as concept:skip
            // so the rust realize omits the else clause.
            Json elseBranch = ifs.getElseStmt()
                    .map(e -> {
                        Json lifted = e instanceof BlockStmt eb ? liftBlock(eb, losses) : liftStatement(e, losses);
                        return isEffectivelyEmpty(lifted) ? skipShape() : lifted;
                    })
                    .orElseGet(this::skipShape);
            return Jcs.object(
                "args", new Jcs.Arr(List.of(cond, thenBranch, elseBranch)),
                "concept_name", Jcs.string("concept:conditional")
            );
        }
        if (stmt instanceof ForEachStmt fes) {
            Json varLeaf = Jcs.object(
                "kind", Jcs.string("symbol"),
                "text", Jcs.string(fes.getVariable().getVariable(0).getNameAsString())
            );
            Json iterable = liftExpression(fes.getIterable(), losses);
            Json body = fes.getBody() instanceof BlockStmt bb ? liftBlock(bb, losses) : liftStatement(fes.getBody(), losses);
            return Jcs.object(
                "args", new Jcs.Arr(List.of(varLeaf, iterable, body)),
                "concept_name", Jcs.string("concept:for-each")
            );
        }
        if (stmt instanceof BreakStmt) {
            return Jcs.object(
                "args", new Jcs.Arr(List.of()),
                "concept_name", Jcs.string("concept:break")
            );
        }
        if (stmt instanceof ContinueStmt) {
            return Jcs.object(
                "args", new Jcs.Arr(List.of()),
                "concept_name", Jcs.string("concept:continue")
            );
        }
        if (stmt instanceof ThrowStmt ts) {
            return Jcs.object(
                "args", new Jcs.Arr(List.of(liftExpression(ts.getExpression(), losses))),
                "concept_name", Jcs.string("concept:throw")
            );
        }
        if (stmt instanceof BlockStmt bs) {
            return liftBlock(bs, losses);
        }
        if (stmt instanceof EmptyStmt) {
            // `;` — no-op (the substrate's lower emits these as `;`
            // placeholders in else-branches with no body).
            return null;
        }
        if (stmt instanceof ReturnStmt rs) {
            return rs.getExpression()
                    .map(expr -> Jcs.object(
                        "args", new Jcs.Arr(List.of(liftExpression(expr, losses))),
                        "concept_name", Jcs.string("concept:return")
                    ))
                    .orElseGet(() -> Jcs.object(
                        "args", new Jcs.Arr(List.of()),
                        "concept_name", Jcs.string("concept:return")
                    ));
        }
        if (stmt instanceof ExpressionStmt es) {
            return liftExpression(es.getExpression(), losses);
        }
        recordLoss(losses, "unrecognized-stmt", stmt);
        return Jcs.object();
    }

    private Json liftExpression(Expression expr, List<Json> losses) {
        Optional<String> cited = readCitation(expr);
        if (cited.isPresent()) {
            return reconstructFromCitation(cited.get(), expr, losses);
        }
        if (expr instanceof StringLiteralExpr s) {
            return Jcs.object(
                "kind", Jcs.string("const"),
                "value", Jcs.string(s.getValue())
            );
        }
        if (expr instanceof IntegerLiteralExpr i) {
            // Emit value as integer (not string) so the rust realize
            // renders it as a numeric literal, not a quoted string.
            return Jcs.object(
                "kind", Jcs.string("const"),
                "value", Jcs.integer(Long.parseLong(i.getValue()))
            );
        }
        if (expr instanceof NameExpr n) {
            return Jcs.object(
                "kind", Jcs.string("symbol"),
                "text", Jcs.string(n.getNameAsString())
            );
        }
        if (expr instanceof BooleanLiteralExpr b) {
            return Jcs.object(
                "kind", Jcs.string("const"),
                "value", Jcs.string(Boolean.toString(b.getValue()))
            );
        }
        if (expr instanceof NullLiteralExpr) {
            return Jcs.object(
                "kind", Jcs.string("const"),
                "value", Jcs.string("null")
            );
        }
        if (expr instanceof VariableDeclarationExpr vde) {
            // `var x = expr` (or `Type x = expr;`) — concept:assign(name, value).
            // Multiple declarators in one expr emit as a seq.
            List<Json> assigns = new ArrayList<>();
            for (var v : vde.getVariables()) {
                Json nameLeaf = Jcs.object(
                    "kind", Jcs.string("symbol"),
                    "text", Jcs.string(v.getNameAsString())
                );
                Json value = v.getInitializer()
                        .map(init -> liftExpression(init, losses))
                        .orElseGet(Jcs::object);
                assigns.add(Jcs.object(
                    "args", new Jcs.Arr(List.of(nameLeaf, value)),
                    "concept_name", Jcs.string("concept:assign")
                ));
            }
            if (assigns.size() == 1) return assigns.get(0);
            return Jcs.object(
                "args", new Jcs.Arr(assigns),
                "concept_name", Jcs.string("concept:seq")
            );
        }
        if (expr instanceof AssignExpr ae) {
            // `target = value` → concept:assign(target, value).
            return Jcs.object(
                "args", new Jcs.Arr(List.of(
                    liftExpression(ae.getTarget(), losses),
                    liftExpression(ae.getValue(), losses))),
                "concept_name", Jcs.string("concept:assign")
            );
        }
        if (expr instanceof ConditionalExpr ce) {
            // Substrate-symmetric recognition: the substrate's java emit
            // for rust's `X.unwrap_or(default)` is `(X != null ? X : default)`
            // (assuming X is Option-like). Recognize this canonical
            // ternary shape and emit concept:call(X, method:unwrap_or, default).
            String condText = ce.getCondition().toString().replaceAll("\\s+", "");
            String thenText = ce.getThenExpr().toString().replaceAll("\\s+", "");
            String elseText = ce.getElseExpr().toString();
            // Pattern: X != null ? X : default → X.unwrap_or(default)
            if (condText.endsWith("!=null") && condText.startsWith(thenText)) {
                Json optShape = liftExpression(ce.getThenExpr(), losses);
                Json defaultShape = liftExpression(ce.getElseExpr(), losses);
                return Jcs.object(
                    "args", new Jcs.Arr(List.of(
                        optShape,
                        methodConceptLeaf("unwrap_or", 1),
                        defaultShape
                    )),
                    "concept_name", Jcs.string("concept:call")
                );
            }
            // Pattern: X != null ? f(X) : default → X.map(|v| f(v)).unwrap_or(default)
            // (closer to source for transforming-then-defaulting Optionals).
            // For now: fall through to plain conditional if not the
            // simple identity-then form.
            return Jcs.object(
                "args", new Jcs.Arr(List.of(
                    liftExpression(ce.getCondition(), losses),
                    liftExpression(ce.getThenExpr(), losses),
                    liftExpression(ce.getElseExpr(), losses))),
                "concept_name", Jcs.string("concept:conditional")
            );
        }
        if (expr instanceof BinaryExpr be) {
            String op = switch (be.getOperator()) {
                case PLUS -> "add"; case MINUS -> "sub";
                case MULTIPLY -> "mul"; case DIVIDE -> "div"; case REMAINDER -> "mod";
                case EQUALS -> "eq"; case NOT_EQUALS -> "ne";
                case LESS -> "lt"; case LESS_EQUALS -> "le";
                case GREATER -> "gt"; case GREATER_EQUALS -> "ge";
                case AND -> "and"; case OR -> "or";
                case BINARY_AND -> "bitand"; case BINARY_OR -> "bitor";
                case XOR -> "bitxor";
                case LEFT_SHIFT -> "shl";
                case SIGNED_RIGHT_SHIFT, UNSIGNED_RIGHT_SHIFT -> "shr";
                default -> null;
            };
            if (op != null) {
                return Jcs.object(
                    "args", new Jcs.Arr(List.of(
                        liftExpression(be.getLeft(), losses),
                        liftExpression(be.getRight(), losses))),
                    "concept_name", Jcs.string("concept:" + op)
                );
            }
        }
        if (expr instanceof UnaryExpr ue) {
            // Negation, not, etc. Map common ones.
            String op = switch (ue.getOperator()) {
                case MINUS -> "neg";
                case LOGICAL_COMPLEMENT -> "not";
                case BITWISE_COMPLEMENT -> "bitnot";
                default -> null;
            };
            if (op != null) {
                return Jcs.object(
                    "args", new Jcs.Arr(List.of(liftExpression(ue.getExpression(), losses))),
                    "concept_name", Jcs.string("concept:" + op)
                );
            }
        }
        if (expr instanceof InstanceOfExpr ioe) {
            // `x instanceof Type` — used by match arm conditions (Ok/Err).
            // Emit as concept:instance-of(value, type_leaf).
            return Jcs.object(
                "args", new Jcs.Arr(List.of(
                    liftExpression(ioe.getExpression(), losses),
                    Jcs.object(
                        "kind", Jcs.string("type"),
                        "text", Jcs.string(ioe.getType().asString())
                    ))),
                "concept_name", Jcs.string("concept:instance-of")
            );
        }
        if (expr instanceof CastExpr cast) {
            // Map java numeric types to rust equivalents:
            //  int → usize (when used as array index, which is the common case)
            //  long → i64; double → f64; char → char.
            String castType = cast.getType().asString();
            String rustCastType = switch (castType) {
                case "int" -> "usize";
                case "long" -> "i64";
                case "double" -> "f64";
                case "float" -> "f32";
                case "char" -> "char";
                default -> castType;
            };
            return Jcs.object(
                "args", new Jcs.Arr(List.of(
                    liftExpression(cast.getExpression(), losses),
                    Jcs.object(
                        "kind", Jcs.string("type"),
                        "text", Jcs.string(rustCastType)
                    ))),
                "concept_name", Jcs.string("concept:cast")
            );
        }
        if (expr instanceof EnclosedExpr enc) {
            // (expr) — transparent; lift inner.
            return liftExpression(enc.getInner(), losses);
        }
        if (expr instanceof ObjectCreationExpr oce) {
            String typeStr = oce.getType().asString();
            // Carrier-aware: new SumVariant(family, variant, payload)
            // → concept:sum-variant-construct (canonical).
            if ((typeStr.endsWith("SumVariant") || typeStr.equals("SumVariant"))
                    && oce.getArguments().size() == 3) {
                return Jcs.object(
                    "args", new Jcs.Arr(List.of(
                        liftExpression(oce.getArgument(0), losses),
                        liftExpression(oce.getArgument(1), losses),
                        liftExpression(oce.getArgument(2), losses))),
                    "concept_name", Jcs.string("concept:sum-variant-construct")
                );
            }
            // catalog #1391: nullary collection constructors are abstractions.
            // new ArrayList<>() → concept:list-create; new HashMap<>() → concept:map-create.
            String pathTypeStr0 = typeStr.replaceFirst("<.*>", "");
            if (oce.getArguments().isEmpty()) {
                if (pathTypeStr0.equals("java.util.ArrayList") || pathTypeStr0.endsWith(".ArrayList") || pathTypeStr0.equals("ArrayList")) {
                    return Jcs.object(
                        "args", new Jcs.Arr(List.of()),
                        "concept_name", Jcs.string("concept:list-create")
                    );
                }
                if (pathTypeStr0.equals("java.util.HashMap") || pathTypeStr0.endsWith(".HashMap") || pathTypeStr0.equals("HashMap")) {
                    return Jcs.object(
                        "args", new Jcs.Arr(List.of()),
                        "concept_name", Jcs.string("concept:map-create")
                    );
                }
            }
            // Map common java types back to rust equivalents for
            // substrate-symmetric closure. `java.util.ArrayList` was the
            // substrate's emit for `Vec`; `java.util.TreeSet` for `BTreeSet`.
            // Strip diamond `<>` (java's inferred-generics) since rust's
            // `::new()` doesn't need it.
            String pathTypeStr = pathTypeStr0;
            String rustType = pathTypeStr;
            if (pathTypeStr.equals("java.util.ArrayList") || pathTypeStr.endsWith(".ArrayList") || pathTypeStr.equals("ArrayList")) {
                rustType = "Vec";
            } else if (pathTypeStr.equals("java.util.TreeSet") || pathTypeStr.endsWith(".TreeSet") || pathTypeStr.equals("TreeSet")) {
                rustType = "BTreeSet";
            } else if (pathTypeStr.equals("java.util.HashMap") || pathTypeStr.endsWith(".HashMap") || pathTypeStr.equals("HashMap")) {
                rustType = "std::collections::HashMap";
            } else if (pathTypeStr.equals("StringBuilder") || pathTypeStr.endsWith(".StringBuilder")) {
                // StringBuilder → String::with_capacity (when N arg) or String::new()
                if (!oce.getArguments().isEmpty()) {
                    Json argShape = liftExpression(oce.getArgument(0), losses);
                    return Jcs.object(
                        "args", new Jcs.Arr(List.of(
                            Jcs.object("kind", Jcs.string("path"), "text", Jcs.string("String::with_capacity")),
                            argShape
                        )),
                        "concept_name", Jcs.string("concept:call")
                    );
                }
                rustType = "String";
            }
            // Default: `new Type(args)` → concept:call with ::new path leaf.
            List<Json> args = new ArrayList<>();
            args.add(Jcs.object(
                "kind", Jcs.string("path"),
                "text", Jcs.string(rustType + "::new")
            ));
            for (Expression a : oce.getArguments()) {
                args.add(liftExpression(a, losses));
            }
            return Jcs.object(
                "args", new Jcs.Arr(args),
                "concept_name", Jcs.string("concept:call")
            );
        }
        if (expr instanceof ArrayCreationExpr ace) {
            // `new T[] {a, b, c}` — concept:array-literal.
            List<Json> elems = new ArrayList<>();
            ace.getInitializer().ifPresent(init -> {
                for (Expression v : init.getValues()) {
                    elems.add(liftExpression(v, losses));
                }
            });
            return Jcs.object(
                "args", new Jcs.Arr(elems),
                "concept_name", Jcs.string("concept:array-literal")
            );
        }
        if (expr instanceof FieldAccessExpr fae) {
            // `recv.field` — concept:field(recv, field_leaf).
            return Jcs.object(
                "args", new Jcs.Arr(List.of(
                    liftExpression(fae.getScope(), losses),
                    Jcs.object(
                        "kind", Jcs.string("field"),
                        "text", Jcs.string(fae.getNameAsString())
                    ))),
                "concept_name", Jcs.string("concept:field")
            );
        }
        if (expr instanceof ArrayAccessExpr aae) {
            // `arr[idx]` → concept:index(arr, idx).
            return Jcs.object(
                "args", new Jcs.Arr(List.of(
                    liftExpression(aae.getName(), losses),
                    liftExpression(aae.getIndex(), losses))),
                "concept_name", Jcs.string("concept:index")
            );
        }
        if (expr instanceof MethodReferenceExpr mre) {
            // `Type::method` — emit as path-leaf with rust-canonical form.
            // Map common java idioms back to rust:
            //   JsonNode::asText → Value::as_str  (was rust source)
            //   String::valueOf → str::to_string  (rust closure shorthand)
            //   Objects::nonNull → (handled by filter recognizer, but
            //                       emit as canonical text for fallback)
            String scope = mre.getScope().toString();
            String ident = mre.getIdentifier();
            String javaPath = scope + "::" + ident;
            String rustPath = javaPath;
            // Strip java FQN prefixes — drop com.fasterxml.jackson.databind.
            String shortScope = scope;
            if (shortScope.contains(".")) {
                shortScope = shortScope.substring(shortScope.lastIndexOf('.') + 1);
            }
            if (shortScope.equals("JsonNode") && ident.equals("asText")) {
                rustPath = "Value::as_str";
            } else if (shortScope.equals("JsonNode") && ident.equals("asArray")) {
                rustPath = "Value::as_array";
            } else if (shortScope.equals("String") && ident.equals("valueOf")) {
                rustPath = "str::to_string";
            } else if (shortScope.equals("Objects") && ident.equals("nonNull")) {
                // Used by filter chains — map to the rust idiom.
                rustPath = "Option::is_some";
            } else if (scope.contains(".")) {
                // Generic FQN strip: java.util.X::y → X::y
                rustPath = shortScope + "::" + ident;
            }
            return Jcs.object(
                "kind", Jcs.string("path"),
                "text", Jcs.string(rustPath)
            );
        }
        if (expr instanceof LambdaExpr lam) {
            // (params) -> body  → concept:closure(body, p1, p2, ...).
            List<Json> args = new ArrayList<>();
            Json body = lam.getExpressionBody()
                    .map(e -> liftExpression(e, losses))
                    .orElseGet(() -> lam.getBody() instanceof BlockStmt bb ? liftBlock(bb, losses) : Jcs.object());
            args.add(body);
            for (Parameter p : lam.getParameters()) {
                args.add(Jcs.object(
                    "kind", Jcs.string("symbol"),
                    "text", Jcs.string(p.getNameAsString())
                ));
            }
            return Jcs.object(
                "args", new Jcs.Arr(args),
                "concept_name", Jcs.string("concept:closure")
            );
        }
        if (expr instanceof MethodCallExpr m) {
            // Carrier-aware recognition: substrate-emitted carrier
            // factory calls produce canonical concepts. The syntax-
            // driven and citation-driven paths must converge — same
            // concept_name for the same emitted construct.
            String scopeText = m.getScope().map(Object::toString).orElse("");
            String name = m.getNameAsString();
            // json!{} → Supplier-closure pattern recognition.
            // Substrate-symmetric lift: when java emit form was
            // `((Supplier<X>) () -> { var __obj = MAPPER.createObjectNode();
            //   __obj.put(K,V); ... return __obj; }).get()`,
            // recognize it as concept:macro-call(json, ...). Closes the
            // substrate-symmetric cycle for this pattern without needing
            // the @substrate-term-shape sidecar.
            if ("get".equals(name) && m.getArguments().isEmpty()) {
                Optional<Jcs.Json> jsonMacro = tryRecognizeJsonSupplier(m);
                if (jsonMacro.isPresent()) {
                    return jsonMacro.get();
                }
            }
            // java.util.Objects.equals(a, b) → a == b (rust equality).
            if ("equals".equals(name) && scopeText.endsWith("Objects") && m.getArguments().size() == 2) {
                Json a = liftExpression(m.getArgument(0), losses);
                Json b = liftExpression(m.getArgument(1), losses);
                return Jcs.object(
                    "args", new Jcs.Arr(List.of(a, b)),
                    "concept_name", Jcs.string("concept:eq")
                );
            }
            // java.nio.file.Path.of(X) → PathBuf::from(X).
            if ("of".equals(name) && scopeText.endsWith("Path") && m.getArguments().size() == 1) {
                Json argShape = liftExpression(m.getArgument(0), losses);
                return Jcs.object(
                    "args", new Jcs.Arr(List.of(
                        Jcs.object("kind", Jcs.string("path"), "text", Jcs.string("PathBuf::from")),
                        argShape
                    )),
                    "concept_name", Jcs.string("concept:call")
                );
            }
            // String.format(fmt, args...) → rust's format! macro.
            if ("format".equals(name) && scopeText.equals("String") && !m.getArguments().isEmpty()) {
                // Convert java fmt (%s, %d) → rust fmt ({}, {}) in the first arg.
                Expression fmtArg = m.getArgument(0);
                String fmtText = fmtArg.toString();
                // Strip surrounding quotes if string literal.
                String inner = fmtText;
                if (inner.startsWith("\"") && inner.endsWith("\"")) {
                    inner = inner.substring(1, inner.length() - 1);
                }
                // Java specifiers → rust:
                inner = inner.replace("%s", "{}").replace("%d", "{}").replace("%i", "{}");
                String rustFmt = "\"" + inner + "\"";
                // Build the macro body as `"fmt", arg1, arg2`.
                StringBuilder body = new StringBuilder(rustFmt);
                for (int i = 1; i < m.getArguments().size(); i++) {
                    body.append(", ").append(m.getArgument(i).toString());
                }
                return Jcs.object(
                    "args", new Jcs.Arr(List.of(
                        Jcs.object("kind", Jcs.string("symbol"), "text", Jcs.string("format")),
                        Jcs.object("kind", Jcs.string("symbol"), "text", Jcs.string(body.toString()))
                    )),
                    "concept_name", Jcs.string("concept:macro-call")
                );
            }
            // .getBytes(StandardCharsets.UTF_8) → concept:utf8-encode(recv)
            // catalog: concept:utf8-encode->java:string-getBytes-utf8 (#1391)
            if ("getBytes".equals(name) && m.getArguments().size() == 1
                    && m.getArgument(0).toString().contains("StandardCharsets")
                    && m.getScope().isPresent()) {
                Json recvShape = liftExpression(m.getScope().get(), losses);
                return Jcs.object(
                    "args", new Jcs.Arr(List.of(recvShape)),
                    "concept_name", Jcs.string("concept:utf8-encode")
                );
            }
            // .length() → .len()
            if ("length".equals(name) && m.getArguments().isEmpty() && m.getScope().isPresent()) {
                Json recvShape = liftExpression(m.getScope().get(), losses);
                return Jcs.object(
                    "args", new Jcs.Arr(List.of(
                        recvShape,
                        methodConceptLeaf("len", 0)
                    )),
                    "concept_name", Jcs.string("concept:call")
                );
            }
            // Java collection method name maps to rust equivalents.
            // List.add(x) → Vec::push(x); Set.add(x) → BTreeSet::insert(x).
            // The substrate's java emit doesn't carry the receiver type,
            // so we use a conservative mapping: when scope is a known
            // Vec-like binding, use .push; else .insert. For now: emit
            // both as .push (Vec) or .insert (Set) via a simple name-
            // based heuristic. The receiver's binding name typically
            // hints the type (ir_entries → Vec; seen_names → BTreeSet).
            if ("add".equals(name) && m.getScope().isPresent() && m.getArguments().size() == 1) {
                String recvName = m.getScope().get().toString();
                String mname;
                // Conservative heuristic: receivers ending in `_names` or
                // `_set` are sets (BTreeSet); others are Vecs.
                if (recvName.endsWith("_names") || recvName.endsWith("_set")
                        || recvName.endsWith("Names") || recvName.endsWith("Set")) {
                    mname = "insert";
                } else {
                    mname = "push";
                }
                Json recvShape = liftExpression(m.getScope().get(), losses);
                Json argShape = liftExpression(m.getArgument(0), losses);
                return Jcs.object(
                    "args", new Jcs.Arr(List.of(
                        recvShape,
                        methodConceptLeaf(mname, 1),
                        argShape
                    )),
                    "concept_name", Jcs.string("concept:call")
                );
            }
            // StringBuilder.append(X) → String's push_str (for &str) or push (for char).
            // The substrate's String::with_capacity path needs `let mut s = ...`
            // and these calls modify it.
            if ("append".equals(name) && m.getScope().isPresent() && m.getArguments().size() == 1) {
                Json recvShape = liftExpression(m.getScope().get(), losses);
                Json argShape = liftExpression(m.getArgument(0), losses);
                // Detect char-typed arg: java cast `(char) X` or `char X`.
                com.github.javaparser.ast.expr.Expression argE = m.getArgument(0);
                boolean isChar = false;
                if (argE instanceof com.github.javaparser.ast.expr.CastExpr c) {
                    isChar = "char".equals(c.getType().asString());
                }
                String mname = isChar ? "push" : "push_str";
                return Jcs.object(
                    "args", new Jcs.Arr(List.of(
                        recvShape,
                        methodConceptLeaf(mname, 1),
                        argShape
                    )),
                    "concept_name", Jcs.string("concept:call")
                );
            }
            // StringBuilder.toString() — when called on a String accumulator,
            // drop the call (rust source has just the tail expression).
            if ("toString".equals(name) && m.getArguments().isEmpty() && m.getScope().isPresent()) {
                return liftExpression(m.getScope().get(), losses);
            }
            // catalog #1391: instance methods that realize concept hubs.
            if (m.getScope().isPresent() && m.getArguments().isEmpty()) {
                String abstraction = null;
                switch (name) {
                    case "asText": abstraction = "concept:json-text-coerce"; break;
                    default: break;
                }
                if (abstraction != null) {
                    Json recvShape = liftExpression(m.getScope().get(), losses);
                    return Jcs.object(
                        "args", new Jcs.Arr(List.of(recvShape)),
                        "concept_name", Jcs.string(abstraction)
                    );
                }
            }
            // catalog #1391: Objects.nonNull(x) → concept:option-is-some(x).
            if ("nonNull".equals(name) && m.getArguments().size() == 1
                    && m.getScope().isPresent()
                    && m.getScope().get().toString().endsWith("Objects")) {
                Json argShape = liftExpression(m.getArgument(0), losses);
                return Jcs.object(
                    "args", new Jcs.Arr(List.of(argShape)),
                    "concept_name", Jcs.string("concept:option-is-some")
                );
            }
            // Jackson JsonNode + java String method names → rust equivalents.
            // These map 1:1 between the substrate's emit and source idiom.
            String rustMethodName = null;
            if (m.getScope().isPresent() && m.getArguments().isEmpty()) {
                switch (name) {
                    case "asText": rustMethodName = "as_str"; break;
                    case "asLong": rustMethodName = "as_i64"; break;
                    case "asInt": rustMethodName = "as_i64"; break;
                    case "asDouble": rustMethodName = "as_f64"; break;
                    case "asBoolean": rustMethodName = "as_bool"; break;
                    case "isNull": rustMethodName = "is_null"; break;
                    case "isArray": rustMethodName = "is_array"; break;
                    case "isObject": rustMethodName = "is_object"; break;
                    case "isString": rustMethodName = "is_string"; break;
                    case "isEmpty": rustMethodName = "is_empty"; break;
                    case "toString": rustMethodName = null; break; // already handled
                }
            }
            if (rustMethodName != null) {
                Json recvShape = liftExpression(m.getScope().get(), losses);
                return Jcs.object(
                    "args", new Jcs.Arr(List.of(
                        recvShape,
                        methodConceptLeaf(rustMethodName, 0)
                    )),
                    "concept_name", Jcs.string("concept:call")
                );
            }
            // Function.apply(X) → method call on X. The substrate emits
            // `((Function) m_ref).apply(arg)` for some translations;
            // unwrap to just calling m_ref(arg) directly.
            if ("apply".equals(name) && m.getScope().isPresent() && m.getArguments().size() == 1) {
                com.github.javaparser.ast.expr.Expression scope = m.getScope().get();
                while (scope instanceof com.github.javaparser.ast.expr.EnclosedExpr enc) {
                    scope = enc.getInner();
                }
                while (scope instanceof com.github.javaparser.ast.expr.CastExpr cast) {
                    scope = cast.getExpression();
                }
                // If scope is a MethodReferenceExpr (e.g. Value::as_str),
                // emit as method call: arg.method()
                if (scope instanceof com.github.javaparser.ast.expr.MethodReferenceExpr mref) {
                    String mname = mref.getIdentifier();
                    Json argShape = liftExpression(m.getArgument(0), losses);
                    return Jcs.object(
                        "args", new Jcs.Arr(List.of(
                            argShape,
                            methodConceptLeaf(mname, 0)
                        )),
                        "concept_name", Jcs.string("concept:call")
                    );
                }
            }
            // Iterator chain recognition: rust `X.iter().filter_map(c).collect()`
            // lowers to java `StreamSupport.stream(X.spliterator(), false)
            //   .map(c).filter(Objects::nonNull).collect(Collectors.toList())`.
            // Detect the canonical .collect(Collectors.toList()) form +
            // walk back through the chain.
            if ("collect".equals(name) && m.getArguments().size() == 1) {
                Optional<Jcs.Json> iterChain = tryRecognizeIteratorChain(m, losses);
                if (iterChain.isPresent()) {
                    return iterChain.get();
                }
            }
            // com.provekit.runtime.Substrate.X — the runtime helpers
            // that carry concept identity at runtime. Both the citation
            // path and the syntax path produce the same canonical concept.
            if (scopeText.endsWith("Substrate") || scopeText.equals("Substrate")
                    || scopeText.endsWith("com.provekit.runtime.Substrate")) {
                if ("cloneOf".equals(name) && m.getArguments().size() == 1) {
                    return Jcs.object(
                        "args", new Jcs.Arr(List.of(liftExpression(m.getArgument(0), losses))),
                        "concept_name", Jcs.string("concept:value-clone")
                    );
                }
                if ("tryUnwrap".equals(name) && m.getArguments().size() == 1) {
                    // Substrate-canonical: concept:try (rust source-form
                    // `expr?`). The rust realize emits `?`; java realize
                    // emits Substrate.tryUnwrap. Same concept, two surfaces.
                    return Jcs.object(
                        "args", new Jcs.Arr(List.of(liftExpression(m.getArgument(0), losses))),
                        "concept_name", Jcs.string("concept:try")
                    );
                }
                if ("unreachable".equals(name) && m.getArguments().size() == 1) {
                    return Jcs.object(
                        "args", new Jcs.Arr(List.of(liftExpression(m.getArgument(0), losses))),
                        "concept_name", Jcs.string("concept:exhaustive-match-no-default")
                    );
                }
            }
            // com.provekit.runtime.Result.ok(x)  → concept:fallible-ok
            // com.provekit.runtime.Result.err(x) → concept:fallible-err
            // Result.okOrElse(value, errSupplier) → concept:fallible-ok-or-else
            if (scopeText.endsWith("Result") || scopeText.equals("Result")
                    || scopeText.endsWith("com.provekit.runtime.Result")) {
                if ("ok".equals(name) && m.getArguments().size() == 1) {
                    return Jcs.object(
                        "args", new Jcs.Arr(List.of(liftExpression(m.getArgument(0), losses))),
                        "concept_name", Jcs.string("concept:fallible-ok")
                    );
                }
                if ("err".equals(name) && m.getArguments().size() == 1) {
                    return Jcs.object(
                        "args", new Jcs.Arr(List.of(liftExpression(m.getArgument(0), losses))),
                        "concept_name", Jcs.string("concept:fallible-err")
                    );
                }
                if ("okOrElse".equals(name) && m.getArguments().size() == 2) {
                    return Jcs.object(
                        "args", new Jcs.Arr(List.of(
                            liftExpression(m.getArgument(0), losses),
                            liftExpression(m.getArgument(1), losses))),
                        "concept_name", Jcs.string("concept:fallible-ok-or-else")
                    );
                }
            }
            // No scope (free function / static method call without
            // class qualifier) → emit as concept:call(path, args...)
            // matching rust source-form for free function calls.
            if (m.getScope().isEmpty()) {
                List<Json> args = new ArrayList<>();
                args.add(Jcs.object(
                    "kind", Jcs.string("path"),
                    "text", Jcs.string(m.getNameAsString())
                ));
                for (Expression a : m.getArguments()) {
                    args.add(liftExpression(a, losses));
                }
                return Jcs.object(
                    "args", new Jcs.Arr(args),
                    "concept_name", Jcs.string("concept:call")
                );
            }
            // With scope: concept:call(receiver, method-concept-leaf, args...).
            List<Json> args = new ArrayList<>();
            m.getScope().ifPresent(scope -> args.add(liftExpression(scope, losses)));
            args.add(methodConceptLeaf(m.getNameAsString(), m.getArguments().size()));
            for (Expression a : m.getArguments()) {
                args.add(liftExpression(a, losses));
            }
            return Jcs.object(
                "args", new Jcs.Arr(args),
                "concept_name", Jcs.string("concept:call")
            );
        }
        recordLoss(losses, "unrecognized-expr", expr);
        return Jcs.object();
    }

    /** Try to recognize the json!-macro emission pattern in java:
     *  `((Supplier&lt;X&gt;) () -> { ObjectNode __obj = MAPPER.createObjectNode();
     *    __obj.put(K, V); ... return __obj; }).get()`
     *
     *  When matched, returns a concept:macro-call node mirroring what
     *  the rust lift would have emitted from `json!{ K: V, ... }`. Closes
     *  the substrate-symmetric cycle for this pattern.
     *
     *  Approach: walk the .get() receiver looking for a cast →
     *  lambda(no params) → block body matching the createObjectNode +
     *  put chain. Reconstruct the K:V pairs from the put() calls and
     *  emit them as concept:macro-call args.
     */
    private Optional<Jcs.Json> tryRecognizeJsonSupplier(MethodCallExpr getCall) {
        return tryRecognizeJsonSupplier(getCall, /*nested=*/false);
    }

    private Optional<Jcs.Json> tryRecognizeJsonSupplier(MethodCallExpr getCall, boolean nested) {
        var scope = getCall.getScope();
        if (scope.isEmpty()) return Optional.empty();
        // Unwrap outer parens.
        com.github.javaparser.ast.expr.Expression inner = scope.get();
        while (inner instanceof com.github.javaparser.ast.expr.EnclosedExpr enc) {
            inner = enc.getInner();
        }
        // Expect a CastExpr to Supplier<X>.
        if (!(inner instanceof com.github.javaparser.ast.expr.CastExpr cast)) {
            return Optional.empty();
        }
        String castType = cast.getType().asString();
        if (!castType.contains("Supplier")) return Optional.empty();
        // Cast operand is the lambda.
        com.github.javaparser.ast.expr.Expression lambdaExpr = cast.getExpression();
        while (lambdaExpr instanceof com.github.javaparser.ast.expr.EnclosedExpr enc) {
            lambdaExpr = enc.getInner();
        }
        if (!(lambdaExpr instanceof com.github.javaparser.ast.expr.LambdaExpr lambda)) {
            return Optional.empty();
        }
        if (!lambda.getParameters().isEmpty()) return Optional.empty();
        // Body should be a Block.
        if (!(lambda.getBody() instanceof com.github.javaparser.ast.stmt.BlockStmt block)) {
            return Optional.empty();
        }
        // Walk block: expect var __obj = MAPPER.createObjectNode(); puts; return __obj
        java.util.List<com.github.javaparser.ast.stmt.Statement> stmts = block.getStatements();
        if (stmts.size() < 2) return Optional.empty();
        // First stmt: var binding to createObjectNode call.
        com.github.javaparser.ast.stmt.Statement first = stmts.get(0);
        String objVar = null;
        if (first instanceof com.github.javaparser.ast.stmt.ExpressionStmt es
                && es.getExpression() instanceof com.github.javaparser.ast.expr.VariableDeclarationExpr vde) {
            for (var v : vde.getVariables()) {
                if (v.getInitializer().isPresent()
                        && v.getInitializer().get() instanceof MethodCallExpr init
                        && "createObjectNode".equals(init.getNameAsString())) {
                    objVar = v.getNameAsString();
                    break;
                }
            }
        }
        if (objVar == null) return Optional.empty();
        // Collect put(K, V) and set(K, V) pairs.
        java.util.List<String> kvPairs = new java.util.ArrayList<>();
        for (int i = 1; i < stmts.size() - 1; i++) {
            com.github.javaparser.ast.stmt.Statement s = stmts.get(i);
            if (!(s instanceof com.github.javaparser.ast.stmt.ExpressionStmt ese)) continue;
            if (!(ese.getExpression() instanceof MethodCallExpr putCall)) continue;
            String putName = putCall.getNameAsString();
            if (!"put".equals(putName) && !"set".equals(putName)) continue;
            if (putCall.getArguments().size() != 2) continue;
            // Detect nested json! macros (nested objects): the value arg
            // could itself be a Supplier-closure .get() call — recurse.
            com.github.javaparser.ast.expr.Expression keyExpr = putCall.getArgument(0);
            com.github.javaparser.ast.expr.Expression valExpr = putCall.getArgument(1);
            String key = keyExpr.toString();
            String value;
            if (valExpr instanceof MethodCallExpr nestedGet
                    && "get".equals(nestedGet.getNameAsString())
                    && nestedGet.getArguments().isEmpty()) {
                Optional<Jcs.Json> nestedCall = tryRecognizeJsonSupplier(nestedGet, /*nested=*/true);
                if (nestedCall.isPresent()) {
                    // Render nested as its inner token form.
                    value = renderJsonMacroBody(nestedCall.get());
                } else {
                    // Could be array-Supplier — recognize createArrayNode pattern.
                    Optional<String> arr = tryRecognizeArraySupplier(nestedGet);
                    value = arr.orElse(valExpr.toString());
                }
            } else if (valExpr instanceof MethodCallExpr valToTree
                    && "valueToTree".equals(valToTree.getNameAsString())
                    && valToTree.getArguments().size() == 1) {
                // MAPPER.valueToTree(X) is the substrate's primitive
                // wrapper for non-JsonNode values in json!{}. Unwrap to
                // just X (source had a bare value there).
                value = valToTree.getArgument(0).toString();
            } else {
                value = valExpr.toString();
            }
            kvPairs.add(key + ": " + value);
        }
        // Reconstruct json! body tokens. Source-style: outer-level
        // (multi-line in source) gets trailing comma + space; nested
        // inline objects omit trailing comma.
        StringBuilder body = new StringBuilder("{ ");
        for (int i = 0; i < kvPairs.size(); i++) {
            body.append(kvPairs.get(i));
            if (i + 1 < kvPairs.size()) {
                body.append(", ");
            }
        }
        // Trailing-comma heuristic: source convention has trailing comma
        // on multi-line layouts (3+ items OR body > 60 chars) and no
        // trailing comma on small inline objects. Matches what the rust
        // pretty-printer expects to decide layout style.
        boolean wouldBeMultiLine = !nested || kvPairs.size() >= 3 || body.length() > 60;
        if (wouldBeMultiLine) {
            body.append(", }");
        } else {
            body.append(" }");
        }
        Jcs.Json pathLeaf = Jcs.object(
            "kind", Jcs.string("symbol"),
            "text", Jcs.string("json")
        );
        Jcs.Json tokensLeaf = Jcs.object(
            "kind", Jcs.string("symbol"),
            "text", Jcs.string(body.toString())
        );
        return Optional.of(Jcs.object(
            "args", new Jcs.Arr(java.util.List.of(pathLeaf, tokensLeaf)),
            "concept_name", Jcs.string("concept:macro-call")
        ));
    }

    /** Recognize rust's `X.iter().filter_map(c).collect()` chain from the
     *  java emission `StreamSupport.stream(X.spliterator(), false)
     *  .map(c).filter(Objects::nonNull).collect(Collectors.toList())`.
     *  Returns concept:call chain mirroring the rust source form. */
    private Optional<Jcs.Json> tryRecognizeIteratorChain(MethodCallExpr collectCall, List<Json> losses) {
        // Verify collect arg is Collectors.toList() (or similar).
        String collectArg = collectCall.getArgument(0).toString();
        if (!collectArg.contains("Collectors.toList") && !collectArg.contains("Collectors.toUnmodifiableList")) {
            return Optional.empty();
        }
        // Receiver of collect: should be .filter(...) of .map(...) of StreamSupport.stream(...).
        com.github.javaparser.ast.expr.Expression recv = collectCall.getScope().orElse(null);
        if (!(recv instanceof MethodCallExpr filterCall)) return Optional.empty();
        if (!"filter".equals(filterCall.getNameAsString())) return Optional.empty();
        com.github.javaparser.ast.expr.Expression filterRecv = filterCall.getScope().orElse(null);
        if (!(filterRecv instanceof MethodCallExpr mapCall)) return Optional.empty();
        if (!"map".equals(mapCall.getNameAsString())) return Optional.empty();
        com.github.javaparser.ast.expr.Expression mapRecv = mapCall.getScope().orElse(null);
        if (!(mapRecv instanceof MethodCallExpr streamCall)) return Optional.empty();
        if (!"stream".equals(streamCall.getNameAsString())) return Optional.empty();
        String streamScope = streamCall.getScope().map(Object::toString).orElse("");
        if (!streamScope.contains("StreamSupport")) return Optional.empty();
        // Stream args: (X.spliterator(), false). Extract X.
        if (streamCall.getArguments().isEmpty()) return Optional.empty();
        com.github.javaparser.ast.expr.Expression splitArg = streamCall.getArgument(0);
        // Unwrap any casts.
        while (splitArg instanceof com.github.javaparser.ast.expr.CastExpr cast) {
            splitArg = cast.getExpression();
        }
        if (!(splitArg instanceof MethodCallExpr splitCall)) return Optional.empty();
        if (!"spliterator".equals(splitCall.getNameAsString())) return Optional.empty();
        // The source data is splitCall's receiver.
        com.github.javaparser.ast.expr.Expression sourceExpr = splitCall.getScope().orElse(null);
        if (sourceExpr == null) return Optional.empty();
        // The map closure is the filter_map closure.
        com.github.javaparser.ast.expr.Expression mapClosure = mapCall.getArgument(0);
        // Reconstruct as concept:call chain: collect(filter_map(iter(source), closure))
        Jcs.Json sourceShape = liftExpression(sourceExpr, losses);
        // .iter() leaf wrapper:
        Jcs.Json iterChain = Jcs.object(
            "args", new Jcs.Arr(java.util.List.of(
                sourceShape,
                methodConceptLeaf("iter", 0)
            )),
            "concept_name", Jcs.string("concept:call")
        );
        // .filter_map(closure) chain (skip the explicit filter step —
        // rust's filter_map IS map-then-filter-non-null fused).
        Jcs.Json closureShape = liftExpression(mapClosure, losses);
        Jcs.Json filterMapChain = Jcs.object(
            "args", new Jcs.Arr(java.util.List.of(
                iterChain,
                methodConceptLeaf("filter_map", 1),
                closureShape
            )),
            "concept_name", Jcs.string("concept:call")
        );
        // .collect():
        return Optional.of(Jcs.object(
            "args", new Jcs.Arr(java.util.List.of(
                filterMapChain,
                methodConceptLeaf("collect", 0)
            )),
            "concept_name", Jcs.string("concept:call")
        ));
    }

    /** Recognize the array-Supplier emission pattern:
     *  `((Supplier<JsonNode>) () -> { var __arr = MAPPER.createArrayNode();
     *  __arr.add(X); ... return __arr; }).get()` → rust source form
     *  `[X, ...]`. Returns the source-form `[a, b, c]` string if matched. */
    private Optional<String> tryRecognizeArraySupplier(MethodCallExpr getCall) {
        com.github.javaparser.ast.expr.Expression inner = getCall.getScope().orElse(null);
        if (inner == null) return Optional.empty();
        while (inner instanceof com.github.javaparser.ast.expr.EnclosedExpr enc) {
            inner = enc.getInner();
        }
        if (!(inner instanceof com.github.javaparser.ast.expr.CastExpr cast)) return Optional.empty();
        com.github.javaparser.ast.expr.Expression body = cast.getExpression();
        while (body instanceof com.github.javaparser.ast.expr.EnclosedExpr enc) {
            body = enc.getInner();
        }
        if (!(body instanceof com.github.javaparser.ast.expr.LambdaExpr lambda)) return Optional.empty();
        if (!lambda.getParameters().isEmpty()) return Optional.empty();
        if (!(lambda.getBody() instanceof com.github.javaparser.ast.stmt.BlockStmt block)) return Optional.empty();
        java.util.List<com.github.javaparser.ast.stmt.Statement> stmts = block.getStatements();
        if (stmts.size() < 2) return Optional.empty();
        // First stmt: var __arr = MAPPER.createArrayNode()
        String arrVar = null;
        if (stmts.get(0) instanceof com.github.javaparser.ast.stmt.ExpressionStmt es
                && es.getExpression() instanceof com.github.javaparser.ast.expr.VariableDeclarationExpr vde) {
            for (var v : vde.getVariables()) {
                if (v.getInitializer().isPresent()
                        && v.getInitializer().get() instanceof MethodCallExpr init
                        && "createArrayNode".equals(init.getNameAsString())) {
                    arrVar = v.getNameAsString();
                    break;
                }
            }
        }
        if (arrVar == null) return Optional.empty();
        // Collect add(X) values.
        java.util.List<String> values = new java.util.ArrayList<>();
        for (int i = 1; i < stmts.size() - 1; i++) {
            if (!(stmts.get(i) instanceof com.github.javaparser.ast.stmt.ExpressionStmt ese)) continue;
            if (!(ese.getExpression() instanceof MethodCallExpr addCall)) continue;
            if (!"add".equals(addCall.getNameAsString())) continue;
            if (addCall.getArguments().size() != 1) continue;
            values.add(addCall.getArgument(0).toString());
        }
        StringBuilder out = new StringBuilder("[");
        for (int i = 0; i < values.size(); i++) {
            out.append(values.get(i));
            if (i + 1 < values.size()) out.append(", ");
        }
        out.append("]");
        return Optional.of(out.toString());
    }

    /** Render a concept:macro-call node back to its body-token form for
     *  use as a nested value in another json! reconstruction. */
    private String renderJsonMacroBody(Jcs.Json macroCall) {
        if (!(macroCall instanceof Jcs.Obj obj)) return "";
        for (Jcs.Field f : obj.fields()) {
            if ("args".equals(f.key()) && f.value() instanceof Jcs.Arr arr
                    && arr.values().size() >= 2) {
                Jcs.Json tokensLeaf = arr.values().get(1);
                if (tokensLeaf instanceof Jcs.Obj t) {
                    for (Jcs.Field tf : t.fields()) {
                        if ("text".equals(tf.key()) && tf.value() instanceof Jcs.Str s) {
                            return s.value();
                        }
                    }
                }
            }
        }
        return "";
    }

    /** Read a leading {@code /*@concept ...*}{@code /} citation comment
     *  from an AST node. Returns the citation body (without delimiters). */
    private Optional<String> readCitation(Node node) {
        return node.getComment().filter(Comment::isBlockComment).map(Comment::getContent)
                .map(String::trim)
                .filter(c -> c.startsWith("@concept"));
    }

    /** Reconstruct a ProofIR node from a citation body. Initial version:
     *  parses {@code @concept <name> [k=v ...]} into a synthetic
     *  concept-named operation. Future work: structurally-faithful
     *  reconstruction with payload references. */
    private Json reconstructFromCitation(String citation, Node sourceNode, List<Json> losses) {
        // Parse `@concept <name> [key=value ...]`. The concept name is the
        // canonical concept identity; key=value attrs carry structural
        // discriminators (arity, family, variant, etc.). The substrate's
        // lower side embeds enough attrs to reconstruct.
        String body = citation.substring("@concept".length()).trim();
        String conceptName;
        java.util.Map<String, String> attrs = new java.util.LinkedHashMap<>();
        int firstSpace = body.indexOf(' ');
        if (firstSpace < 0) {
            conceptName = body;
        } else {
            conceptName = body.substring(0, firstSpace).trim();
            String rest = body.substring(firstSpace + 1).trim();
            parseAttrs(rest, attrs);
        }
        // Emit a structurally-faithful concept node: args carry any
        // payload references the substrate cited. For round-trip
        // identity, the SHAPE that emits a citation must match the
        // shape a citation-driven lifter reconstructs.
        List<Json> args = new ArrayList<>();
        // The actual operand expressions are alongside the citation in
        // the source (the citation comments are SIDE annotations). To
        // reconstruct the full ProofIR we still need to lift the
        // associated source expressions — the citation is the IDENTITY
        // not the substitute. So fall through to syntax-driven lift
        // for the operand expressions; the citation just supplies the
        // concept_name + attrs.
        Json structural = liftExpressionSyntactically(sourceNode, losses);
        List<Json> structuralArgs = extractArgs(structural);
        // Convergence rule: citation-driven and syntax-driven lifts must
        // produce byte-identical ProofIR. The citation's concept_name
        // wins; structural args come from lifting the source expression.
        // Citation ATTRS (payload=, family=, etc.) are NOT emitted as
        // separate fields — they're informational redundancy with the
        // structural args, not new ProofIR content. Keeping them would
        // make cited-lift diverge from syntax-lift on the same construct.
        List<Jcs.Field> fields = new ArrayList<>();
        fields.add(new Jcs.Field("args", new Jcs.Arr(structuralArgs)));
        fields.add(new Jcs.Field("concept_name", Jcs.string(conceptName)));
        return new Jcs.Obj(fields);
    }

    /** Parse `key=value [key=value ...]` from a citation body. Values
     *  may contain spaces if no `key=` follows; for now we use a simple
     *  splitter that handles the common emission patterns. */
    private void parseAttrs(String body, java.util.Map<String, String> out) {
        String[] tokens = body.split(" ");
        String currentKey = null;
        StringBuilder currentValue = new StringBuilder();
        for (String tok : tokens) {
            int eq = tok.indexOf('=');
            if (eq > 0 && tok.substring(0, eq).matches("[a-zA-Z_][a-zA-Z0-9_-]*")) {
                if (currentKey != null) out.put(currentKey, currentValue.toString());
                currentKey = tok.substring(0, eq);
                currentValue = new StringBuilder(tok.substring(eq + 1));
            } else if (currentKey != null) {
                currentValue.append(' ').append(tok);
            }
        }
        if (currentKey != null) out.put(currentKey, currentValue.toString());
    }

    /** Extract `args` array from a lifted JSON node, or empty list if
     *  the node has no args field. */
    private List<Json> extractArgs(Json node) {
        if (node instanceof Jcs.Obj obj) {
            for (Jcs.Field f : obj.fields()) {
                if ("args".equals(f.key()) && f.value() instanceof Jcs.Arr arr) {
                    return new ArrayList<>(arr.values());
                }
            }
        }
        return new ArrayList<>();
    }

    /** Syntactic lift WITHOUT the citation short-circuit — used when the
     *  citation reconstruction needs the operand expressions. */
    private Json liftExpressionSyntactically(Node node, List<Json> losses) {
        if (node instanceof Expression e) {
            // Temporarily clear the comment so liftExpression doesn't
            // short-circuit. Re-attach after.
            Optional<Comment> saved = e.getComment();
            e.removeComment();
            try {
                return liftExpression(e, losses);
            } finally {
                saved.ifPresent(e::setComment);
            }
        }
        if (node instanceof Statement s) {
            Optional<Comment> saved = s.getComment();
            s.removeComment();
            try {
                return liftStatement(s, losses);
            } finally {
                saved.ifPresent(s::setComment);
            }
        }
        return Jcs.object();
    }

    private Json methodConceptLeaf(String name, int arity) {
        // Canonical method-concept leaf, structure determines CID.
        return Jcs.object(
            "arity", Jcs.string(Integer.toString(arity)),
            "concept_name", Jcs.string("method:" + name),
            "kind", Jcs.string("method"),
            "text", Jcs.string(name)
        );
    }

    private void recordLoss(List<Json> losses, String dimension, Node node) {
        recordLoss(losses, dimension, node.getClass().getSimpleName()
                + " '" + truncate(node.toString(), 100) + "' at " + nodeLocation(node));
    }

    private void recordLoss(List<Json> losses, String dimension, String detail) {
        losses.add(Jcs.object(
            "dimension", Jcs.string(dimension),
            "detail", Jcs.string(detail),
            "kind", Jcs.string("lift-gap")
        ));
    }

    private String nodeLocation(Node node) {
        return node.getRange().map(r -> r.begin.line + ":" + r.begin.column).orElse("?");
    }

    private String truncate(String s, int max) {
        s = s.replace("\n", " ").replace("\r", " ");
        return s.length() > max ? s.substring(0, max) + "..." : s;
    }
}
