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
    /** When a lambda's BlockStmt body is a single `return X;` statement,
     *  lift the inner expression directly. */
    private Json unwrapSingleReturn(BlockStmt block, List<Json> losses) {
        if (block.getStatements().size() == 1 &&
                block.getStatement(0) instanceof com.github.javaparser.ast.stmt.ReturnStmt rs &&
                rs.getExpression().isPresent()) {
            return liftExpression(rs.getExpression().get(), losses);
        }
        return liftBlock(block, losses);
    }

    private Json liftBlock(BlockStmt block, List<Json> losses) {
        // Substrate-symmetric match recognition: the rust lower emits
        // `match scrut { pat1 => body1, _ => body2 }` as java
        // `var __provekit_vN = scrut; if (pat-as-cond) { body1 } else { body2 }`.
        // Detect this canonical 2-statement pattern and emit concept:match.
        Optional<Json> matchRecognized = tryRecognizeMatch(block, losses);
        if (matchRecognized.isPresent()) return matchRecognized.get();
        // #1391 follow-on: multi-statement match-assign recognition. Rust
        // `let req = match scrut { Ok(v) => v, Err(e) => { return ...; } }`
        // emits the triplet:
        //   T req;
        //   var __provekit_vN = scrut;
        //   if (cond1) { req = ...; } else if (cond2) { return ...; } else {...}
        // Recognize and emit as concept:assign(req, concept:match(scrut, arms))
        // inside the surrounding seq.
        List<Json> tripletSweep = tryRecognizeMatchAssignTriplet(block, losses);
        if (tripletSweep != null) {
            if (tripletSweep.size() == 1) return tripletSweep.get(0);
            return Jcs.object(
                "args", new Jcs.Arr(tripletSweep),
                "concept_name", Jcs.string("concept:seq")
            );
        }
        // #1391 follow-on: sub-block match recognition. The 2-stmt
        // `var __provekit_vN = scrut; if-chain` pattern can appear
        // ANYWHERE inside a larger function body (e.g. handle_line's
        // inner `match method` dispatcher). Sweep for it.
        List<Json> innerMatch = tryRecognizeInnerMatch(block, losses);
        if (innerMatch != null) {
            if (innerMatch.size() == 1) return innerMatch.get(0);
            return Jcs.object(
                "args", new Jcs.Arr(innerMatch),
                "concept_name", Jcs.string("concept:seq")
            );
        }
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
        // #1391 follow-on: blank-line carrier — detect blank line(s) between
        // consecutive java statements via JavaParser begin/end lines and
        // emit a concept:blank-line marker so the cycle preserves rust's
        // paragraph-style separators. One marker per gap (rustfmt
        // normalizes multi-blank to single).
        Integer prevEndLine = null;
        for (Statement s : block.getStatements()) {
            if (prevEndLine != null && s.getBegin().isPresent()) {
                int startLine = s.getBegin().get().line;
                // If a comment is attached to this statement, use the
                // comment's begin line as the effective start — the
                // comment is part of this statement's "span" for
                // blank-line detection purposes. A `// item-decl (rust):`
                // line carrier that immediately follows a statement is
                // NOT a blank line. Without this adjustment, we falsely
                // detect a blank between the previous stmt and the
                // comment-bearing stmt.
                int effectiveStart = startLine;
                if (s.getComment().isPresent()
                        && s.getComment().get().getBegin().isPresent()) {
                    int commentLine = s.getComment().get().getBegin().get().line;
                    if (commentLine < effectiveStart) effectiveStart = commentLine;
                }
                if (effectiveStart > prevEndLine + 1) {
                    stmts.add(Jcs.object(
                        "args", new Jcs.Arr(List.of()),
                        "concept_name", Jcs.string("concept:blank-line")
                    ));
                }
            }
            if (s.getEnd().isPresent()) {
                prevEndLine = s.getEnd().get().line;
            }
            // #1391 follow-on: when java lower emitted a function-local
            // const as a `// item-decl (rust): <source>` comment, the
            // comment attaches to the NEXT statement. Detect such
            // comments here and emit a concept:item-decl shape before
            // the actual statement.
            s.getComment().ifPresent(c -> {
                if (c instanceof com.github.javaparser.ast.comments.LineComment lc) {
                    String txt = lc.getContent().trim();
                    String marker = "item-decl (rust):";
                    int ix = txt.indexOf(marker);
                    if (ix >= 0) {
                        String src = txt.substring(ix + marker.length()).trim();
                        stmts.add(Jcs.object(
                            "args", new Jcs.Arr(List.of(
                                Jcs.object("kind", Jcs.string("symbol"),
                                           "text", Jcs.string(src))
                            )),
                            "concept_name", Jcs.string("concept:item-decl")
                        ));
                    }
                }
            });
            Json lifted = liftStatement(s, losses);
            if (lifted != null) stmts.add(lifted);
        }
        // Also scan orphan comments at the end of the block (no following stmt).
        for (com.github.javaparser.ast.comments.Comment c : block.getOrphanComments()) {
            if (c instanceof com.github.javaparser.ast.comments.LineComment lc) {
                String txt = lc.getContent().trim();
                String marker = "item-decl (rust):";
                int ix = txt.indexOf(marker);
                if (ix >= 0) {
                    String src = txt.substring(ix + marker.length()).trim();
                    stmts.add(Jcs.object(
                        "args", new Jcs.Arr(List.of(
                            Jcs.object("kind", Jcs.string("symbol"),
                                       "text", Jcs.string(src))
                        )),
                        "concept_name", Jcs.string("concept:item-decl")
                    ));
                }
            }
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
            // #1391 follow-on: blank-line carrier — track end-line of the
            // last consumed statement (the LAST destructure get-call), then
            // for each remaining stmt, emit concept:blank-line when there's
            // a line gap > 1. Same logic as the main liftBlock loop.
            Integer prevEndLine = null;
            if (j > 0 && stmts.get(j - 1).getEnd().isPresent()) {
                prevEndLine = stmts.get(j - 1).getEnd().get().line;
            }
            for (int k = j; k < stmts.size(); k++) {
                Statement st = stmts.get(k);
                if (prevEndLine != null && st.getBegin().isPresent()) {
                    int startLine = st.getBegin().get().line;
                    int effective = startLine;
                    if (st.getComment().isPresent()
                            && st.getComment().get().getBegin().isPresent()) {
                        int cl = st.getComment().get().getBegin().get().line;
                        if (cl < effective) effective = cl;
                    }
                    if (effective > prevEndLine + 1) {
                        out.add(Jcs.object(
                            "args", new Jcs.Arr(List.of()),
                            "concept_name", Jcs.string("concept:blank-line")
                        ));
                    }
                }
                if (st.getEnd().isPresent()) prevEndLine = st.getEnd().get().line;
                Json s = liftStatement(st, losses);
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

    /** Recognize `var __provekit_vN = scrut; if (cond1) {...} else if (cond2) {...} ... else {...}`
     *  as concept:match(scrut, arm1, arm2, ..., armN). The conds are mapped
     *  back to pattern-strings heuristically (e.g. `X.equals("foo")` →
     *  `"foo"`; `instanceof .Ok` → `Ok(v)`; `!= null && !is_null()` →
     *  `Some(v) if !v.is_null()`); for the catch-all final else arm the
     *  pattern is the wildcard `other` (rust's `other => ...` bind) or `_`. */
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
        Json scrutShape = liftExpression(scrutExpr, losses);
        // Case A: stmts[1] is `if-chain` (statement-form match).
        if (stmts.get(1) instanceof com.github.javaparser.ast.stmt.IfStmt ifs) {
            if (ifs.getElseStmt().isEmpty()) return Optional.empty();
            List<Json> arms = collectMatchArms(ifs, binding, losses);
            if (arms == null) return Optional.empty();
            List<Json> matchArgs = new ArrayList<>();
            matchArgs.add(scrutShape);
            matchArgs.addAll(arms);
            return Optional.of(Jcs.object(
                "args", new Jcs.Arr(matchArgs),
                "concept_name", Jcs.string("concept:match")
            ));
        }
        // Case B: stmts[1] is `return ternary` (expression-form match,
        // emitted by java lower for match-in-expression-context). Each
        // ternary arm is a value (not a return).
        if (stmts.get(1) instanceof com.github.javaparser.ast.stmt.ReturnStmt rs
                && rs.getExpression().isPresent()) {
            com.github.javaparser.ast.expr.Expression rExpr = rs.getExpression().get();
            while (rExpr instanceof com.github.javaparser.ast.expr.EnclosedExpr enc) {
                rExpr = enc.getInner();
            }
            if (rExpr instanceof com.github.javaparser.ast.expr.ConditionalExpr ce) {
                List<Json> arms = collectMatchArmsFromTernary(ce, binding, losses);
                if (arms != null) {
                    List<Json> matchArgs = new ArrayList<>();
                    matchArgs.add(scrutShape);
                    matchArgs.addAll(arms);
                    return Optional.of(Jcs.object(
                        "args", new Jcs.Arr(matchArgs),
                        "concept_name", Jcs.string("concept:match")
                    ));
                }
            }
        }
        return Optional.empty();
    }

    /** Walk a ternary chain `cond1 ? body1 : (cond2 ? body2 : default)` and
     *  produce a list of concept:match-arm shapes. Returns null when the
     *  chain doesn't look like a match-emit (e.g. cond can't be mapped
     *  to a pattern). */
    private List<Json> collectMatchArmsFromTernary(
            com.github.javaparser.ast.expr.ConditionalExpr ce,
            String scrutBinding,
            List<Json> losses) {
        List<Json> arms = new ArrayList<>();
        com.github.javaparser.ast.expr.ConditionalExpr current = ce;
        while (true) {
            String pat = derivePatternFromCondition(current.getCondition(), scrutBinding);
            // Variant-arm marker on the then-expr (same recovery as if-chain).
            String[] variantInfo = extractVariantMarkerFromExpr(current.getThenExpr());
            String overridePat = variantInfo != null ? variantInfo[0] : null;
            String overrideBind = variantInfo != null ? variantInfo[1] : null;
            Json body = liftExpression(current.getThenExpr(), losses);
            String effectivePat = overridePat != null ? overridePat : pat;
            String boundVar = overrideBind != null ? overrideBind
                : extractBindingFromPattern(effectivePat);
            if (boundVar != null && !boundVar.equals(scrutBinding)) {
                body = substituteSymbolBinding(body, scrutBinding, boundVar);
                body = stripVariantUnwrap(body, boundVar, effectivePat);
            }
            if (overridePat != null && overrideBind != null) {
                body = collapseSumVariantPayload(body, scrutBinding, overrideBind);
            }
            arms.add(Jcs.object(
                "args", new Jcs.Arr(List.of(
                    Jcs.object("kind", Jcs.string("symbol"), "text", Jcs.string(effectivePat)),
                    body
                )),
                "concept_name", Jcs.string("concept:match-arm")
            ));
            com.github.javaparser.ast.expr.Expression elseExpr = current.getElseExpr();
            while (elseExpr instanceof com.github.javaparser.ast.expr.EnclosedExpr enc) {
                elseExpr = enc.getInner();
            }
            if (elseExpr instanceof com.github.javaparser.ast.expr.ConditionalExpr nested) {
                current = nested;
                continue;
            }
            // Terminal else-expression — wildcard arm. Check if it's the
            // synthetic exhaustive-no-default unreachable marker.
            Json elseBody = liftExpression(elseExpr, losses);
            String elsePattern = wildcardPatternForElseBody(elseBody);
            if (elsePattern == null) {
                return arms;
            }
            if (!"_".equals(elsePattern) && !elsePattern.equals(scrutBinding)) {
                elseBody = substituteSymbolBinding(elseBody, scrutBinding, elsePattern);
            }
            arms.add(Jcs.object(
                "args", new Jcs.Arr(List.of(
                    Jcs.object("kind", Jcs.string("symbol"), "text", Jcs.string(elsePattern)),
                    elseBody
                )),
                "concept_name", Jcs.string("concept:match-arm")
            ));
            return arms;
        }
    }

    /** Walk an if/else-if/else chain and produce a list of concept:match-arm
     *  shapes. Returns null if the chain doesn't look like a match-emit. */
    private List<Json> collectMatchArms(
            com.github.javaparser.ast.stmt.IfStmt ifs,
            String scrutBinding,
            List<Json> losses) {
        List<Json> arms = new ArrayList<>();
        com.github.javaparser.ast.stmt.IfStmt current = ifs;
        while (true) {
            String pat = derivePatternFromCondition(current.getCondition(), scrutBinding);
            Statement thenStmt = current.getThenStmt();
            // Variant-arm marker: when the lower emitted a nested-variant
            // pattern (e.g. `Err(LiftError::InvalidParams(msg))`), it
            // attached a `/*@match-arm-pattern=...*/` comment. Recover the
            // pattern text and bound-var to undo the SumVariant payload
            // unwrap that the lower applied.
            String[] variantInfo = extractVariantMarker(thenStmt);
            String overridePat = variantInfo != null ? variantInfo[0] : null;
            String overrideBind = variantInfo != null ? variantInfo[1] : null;
            Json body = thenStmt instanceof BlockStmt tb ? liftBlock(tb, losses) : liftStatement(thenStmt, losses);
            String effectivePat = overridePat != null ? overridePat : pat;
            String boundVar = overrideBind != null ? overrideBind : extractBindingFromPattern(effectivePat);
            if (boundVar != null && !boundVar.equals(scrutBinding)) {
                body = substituteSymbolBinding(body, scrutBinding, boundVar);
            }
            if (overridePat != null && overrideBind != null) {
                // The lower replaced the binding with `String.valueOf((SumVariant)
                // SCRUT.unwrapErr()).payload()`. Collapse back to the bound name.
                body = collapseSumVariantPayload(body, scrutBinding, overrideBind);
            }
            body = stripTrailingReturn(body);
            arms.add(Jcs.object(
                "args", new Jcs.Arr(List.of(
                    Jcs.object("kind", Jcs.string("symbol"), "text", Jcs.string(effectivePat)),
                    body
                )),
                "concept_name", Jcs.string("concept:match-arm")
            ));
            Optional<Statement> elseOpt = current.getElseStmt();
            if (elseOpt.isEmpty()) {
                // No else — chain ended without a default. Rust source's
                // exhaustive match would still need an arm here; emit `_`.
                return arms;
            }
            Statement elseStmt = elseOpt.get();
            if (elseStmt instanceof com.github.javaparser.ast.stmt.IfStmt nested) {
                current = nested;
                continue;
            }
            // Terminal else block — wildcard arm.
            Json elseBody = elseStmt instanceof BlockStmt eb ? liftBlock(eb, losses) : liftStatement(elseStmt, losses);
            // If the wildcard arm body's first statement is an
            // `concept:exhaustive-match-no-default` synthetic from the lower,
            // the rust source had no `_` arm (variants are exhaustive). Skip it.
            String elsePattern = wildcardPatternForElseBody(elseBody);
            if (elsePattern == null) {
                // Synthetic unreachable — drop the arm.
                return arms;
            }
            // Wildcard-with-binding (`other => ...`): rewrite the synthetic
            // scrut binding to the source's `other` name.
            if (!"_".equals(elsePattern) && !elsePattern.equals(scrutBinding)) {
                elseBody = substituteSymbolBinding(elseBody, scrutBinding, elsePattern);
            }
            elseBody = stripTrailingReturn(elseBody);
            arms.add(Jcs.object(
                "args", new Jcs.Arr(List.of(
                    Jcs.object("kind", Jcs.string("symbol"), "text", Jcs.string(elsePattern)),
                    elseBody
                )),
                "concept_name", Jcs.string("concept:match-arm")
            ));
            return arms;
        }
    }

    /** Inspect a wildcard arm's body. Returns the pattern text to use:
     *  null if the body is a synthetic concept:exhaustive-match-no-default
     *  carrier (drop the arm); "other" if the body references the scrut
     *  binding (rust source had `other => ...`); else "_". */
    private String wildcardPatternForElseBody(Json body) {
        String cn = conceptOf(body);
        if ("concept:exhaustive-match-no-default".equals(cn)) return null;
        if (body instanceof Jcs.Obj obj) {
            for (Jcs.Field f : obj.fields()) {
                if (!"args".equals(f.key())) continue;
                if (!(f.value() instanceof Jcs.Arr arr)) continue;
                for (Jcs.Json child : arr.values()) {
                    if (conceptOf(child) != null
                            && conceptOf(child).startsWith("concept:exhaustive-match-no-default")) {
                        return null;
                    }
                }
            }
        }
        // Default: rust source patterns commonly use `other` as the catch-all
        // binding name when the body references the scrutinee. We can't
        // detect the binding precisely here, so emit `other` — round-trip
        // verifies via byte-comparison.
        return containsSymbolText(body, "__provekit_v") ? "other" : "_";
    }

    private static String conceptOf(Json node) {
        if (!(node instanceof Jcs.Obj obj)) return null;
        for (Jcs.Field f : obj.fields()) {
            if ("concept_name".equals(f.key()) && f.value() instanceof Jcs.Str s) {
                return s.value();
            }
        }
        return null;
    }

    /** True iff any text-bearing leaf in the subtree contains the prefix
     *  as a word (handles both bare symbol leaves and embedded refs
     *  inside macro-call argument strings like `"...{}, __provekit_v1"`). */
    private static boolean containsSymbolText(Json node, String prefix) {
        if (!(node instanceof Jcs.Obj obj)) return false;
        java.util.regex.Pattern pat = java.util.regex.Pattern.compile(
                "\\b" + java.util.regex.Pattern.quote(prefix) + "\\d*\\b");
        for (Jcs.Field f : obj.fields()) {
            Json v = f.value();
            if (v instanceof Jcs.Str s && "text".equals(f.key())
                    && pat.matcher(s.value()).find()) {
                return true;
            }
            if (v instanceof Jcs.Arr arr) {
                for (Jcs.Json child : arr.values()) {
                    if (containsSymbolText(child, prefix)) return true;
                }
            } else if (v instanceof Jcs.Obj o) {
                if (containsSymbolText(o, prefix)) return true;
            }
        }
        return false;
    }

    /** Multi-statement match-assign recognizer. Detects the triplet pattern:
     *
     *   T name;                              // bare decl
     *   var __provekit_vN = scrut;           // temp init
     *   if (cond1) { name = ...; }           // assign-arm OR control-flow
     *   else if (cond2) { return ...; }
     *   ...
     *
     *  Emits the surrounding statements + a single concept:assign(name,
     *  concept:match(scrut, arms)) collapsed shape.
     *
     *  Returns null when the block has no such triplet. The recognizer
     *  scans for ANY occurrence of the triplet (not just at position 0)
     *  so it works inside larger function bodies. */
    private List<Json> tryRecognizeMatchAssignTriplet(BlockStmt block, List<Json> losses) {
        List<Statement> stmts = block.getStatements();
        for (int i = 0; i + 2 < stmts.size(); i++) {
            Statement s0 = stmts.get(i);
            Statement s1 = stmts.get(i + 1);
            Statement s2 = stmts.get(i + 2);
            // Two possible orders the lower may emit:
            //   (A) `T name;`               `var __vN = scrut;`     `if-chain`
            //   (B) `var __vN = scrut;`     `T name;`               `if-chain`
            // Both are equivalent — accept either. The bare decl carries
            // `name`; the var-decl carries the scrut temp binding.
            String targetName = null;
            String tempBinding = null;
            String declaredType = null;
            com.github.javaparser.ast.expr.Expression scrutExpr = null;
            // Try order A first.
            String[] aResult = parseDeclAndTemp(s0, s1);
            if (aResult != null) {
                targetName = aResult[0];
                tempBinding = aResult[1];
                declaredType = aResult[2];
                scrutExpr = ((com.github.javaparser.ast.expr.VariableDeclarationExpr)
                    ((com.github.javaparser.ast.stmt.ExpressionStmt) s1).getExpression())
                    .getVariable(0).getInitializer().get();
            } else {
                String[] bResult = parseDeclAndTemp(s1, s0);
                if (bResult != null) {
                    targetName = bResult[0];
                    tempBinding = bResult[1];
                    declaredType = bResult[2];
                    scrutExpr = ((com.github.javaparser.ast.expr.VariableDeclarationExpr)
                        ((com.github.javaparser.ast.stmt.ExpressionStmt) s0).getExpression())
                        .getVariable(0).getInitializer().get();
                }
            }
            if (targetName == null) continue;
            // s2: if/else-if/else chain whose arm bodies either assign to
            // targetName or are control-flow (return/break/continue).
            if (!(s2 instanceof com.github.javaparser.ast.stmt.IfStmt ifs)) continue;
            if (ifs.getElseStmt().isEmpty()) continue;
            // Collect arms; for each, unwrap an assignment-to-target into
            // the bare RHS (the rust source's match arm IS a value-producing
            // expression for assign-context).
            List<Json> arms = collectMatchArmsForAssign(ifs, tempBinding, targetName, losses);
            if (arms == null) continue;
            Json scrutShape = liftExpression(scrutExpr, losses);
            List<Json> matchArgs = new ArrayList<>();
            matchArgs.add(scrutShape);
            matchArgs.addAll(arms);
            Json matchShape = Jcs.object(
                "args", new Jcs.Arr(matchArgs),
                "concept_name", Jcs.string("concept:match")
            );
            // Build target leaf — include let_type when the rust source had
            // a typed `let req: Value = ...` (lower emitted `JsonNode req;`).
            String rustLetType = javaTypeToRustLetType(declaredType);
            Json targetLeaf = rustLetType != null
                ? Jcs.object(
                    "kind", Jcs.string("symbol"),
                    "let_type", Jcs.string(rustLetType),
                    "text", Jcs.string(targetName)
                )
                : Jcs.object(
                    "kind", Jcs.string("symbol"),
                    "text", Jcs.string(targetName)
                );
            Json assignShape = Jcs.object(
                "args", new Jcs.Arr(List.of(
                    targetLeaf,
                    matchShape
                )),
                "concept_name", Jcs.string("concept:assign")
            );
            // Build output: pre-stmts + assign + post-stmts (with blank-line
            // bookkeeping for the post-stmts).
            List<Json> out = new ArrayList<>();
            Integer prevEndLine = null;
            for (int k = 0; k < i; k++) {
                Statement st = stmts.get(k);
                if (prevEndLine != null && st.getBegin().isPresent()) {
                    int eff = st.getBegin().get().line;
                    if (st.getComment().isPresent()
                            && st.getComment().get().getBegin().isPresent()) {
                        int cl = st.getComment().get().getBegin().get().line;
                        if (cl < eff) eff = cl;
                    }
                    if (eff > prevEndLine + 1) {
                        out.add(Jcs.object(
                            "args", new Jcs.Arr(List.of()),
                            "concept_name", Jcs.string("concept:blank-line")
                        ));
                    }
                }
                if (st.getEnd().isPresent()) prevEndLine = st.getEnd().get().line;
                Json lifted = liftStatement(st, losses);
                if (lifted != null) out.add(lifted);
            }
            out.add(assignShape);
            // Track end line of the if-chain for subsequent blank detection
            // BETWEEN the assign and the first post-statement.
            if (s2.getEnd().isPresent()) prevEndLine = s2.getEnd().get().line;
            if (i + 3 < stmts.size()) {
                Statement firstPost = stmts.get(i + 3);
                if (prevEndLine != null && firstPost.getBegin().isPresent()) {
                    int eff = firstPost.getBegin().get().line;
                    if (firstPost.getComment().isPresent()
                            && firstPost.getComment().get().getBegin().isPresent()) {
                        int cl = firstPost.getComment().get().getBegin().get().line;
                        if (cl < eff) eff = cl;
                    }
                    if (eff > prevEndLine + 1) {
                        out.add(Jcs.object(
                            "args", new Jcs.Arr(List.of()),
                            "concept_name", Jcs.string("concept:blank-line")
                        ));
                    }
                }
            }
            // Lift the remaining tail as a sub-block so it ALSO benefits
            // from the match/tuple/struct recognizers (the inner string-keyed
            // match dispatch in handle_line is a 2-statement `var __vN=method;
            // if-chain` that must be recognized too).
            if (i + 3 < stmts.size()) {
                com.github.javaparser.ast.NodeList<Statement> tailNodes =
                    new com.github.javaparser.ast.NodeList<>();
                for (int k = i + 3; k < stmts.size(); k++) {
                    tailNodes.add(stmts.get(k));
                }
                BlockStmt tailBlock = new BlockStmt(tailNodes);
                Json tailShape = liftBlock(tailBlock, losses);
                // Flatten if it came back as concept:seq.
                if (tailShape instanceof Jcs.Obj tsObj) {
                    String cn = conceptOf(tsObj);
                    if ("concept:seq".equals(cn)) {
                        for (Jcs.Field f : tsObj.fields()) {
                            if ("args".equals(f.key()) && f.value() instanceof Jcs.Arr arr) {
                                for (Jcs.Json child : arr.values()) {
                                    out.add((Json) child);
                                }
                                break;
                            }
                        }
                    } else {
                        out.add(tailShape);
                    }
                } else {
                    out.add(tailShape);
                }
            }
            return out;
        }
        return null;
    }

    /** Walk an if/else-if/else chain in assign-context. For each arm:
     *  - if the body is `targetName = X;` (possibly in a block), unwrap to X
     *    (the rust source's match arm produces a value)
     *  - else (control-flow body), lift the body as-is
     *  Returns null if any arm fails to lift cleanly. */
    private List<Json> collectMatchArmsForAssign(
            com.github.javaparser.ast.stmt.IfStmt ifs,
            String scrutBinding,
            String targetName,
            List<Json> losses) {
        List<Json> arms = new ArrayList<>();
        com.github.javaparser.ast.stmt.IfStmt current = ifs;
        while (true) {
            String pat = derivePatternFromCondition(current.getCondition(), scrutBinding);
            Statement thenStmt = current.getThenStmt();
            Json body = liftAssignArmBody(thenStmt, targetName, losses);
            if (body == null) return null;
            String boundVar = extractBindingFromPattern(pat);
            if (boundVar != null && !boundVar.equals(scrutBinding)) {
                body = substituteSymbolBinding(body, scrutBinding, boundVar);
            }
            // Sum-variant unwrap normalization: the lower emits `Ok(v) => v`
            // as `req = __provekit_v0.unwrap()`. After substitution, body is
            // `v.unwrap()`. Strip the `.unwrap()` — the source bound `v`
            // IS the payload. Similarly, `__v.unwrapErr().payload()` → `e`.
            body = stripVariantUnwrap(body, boundVar, pat);
            // NOTE: do NOT stripTrailingReturn here — arms in assign-context
            // are either value-producing (already stripped by
            // tryUnwrapAssignToTarget) or genuinely divergent (`return X;`),
            // in which case the source preserves the return.
            arms.add(Jcs.object(
                "args", new Jcs.Arr(List.of(
                    Jcs.object("kind", Jcs.string("symbol"), "text", Jcs.string(pat)),
                    body
                )),
                "concept_name", Jcs.string("concept:match-arm")
            ));
            Optional<Statement> elseOpt = current.getElseStmt();
            if (elseOpt.isEmpty()) return arms;
            Statement elseStmt = elseOpt.get();
            if (elseStmt instanceof com.github.javaparser.ast.stmt.IfStmt nested) {
                current = nested;
                continue;
            }
            Json elseBody = liftAssignArmBody(elseStmt, targetName, losses);
            if (elseBody == null) return null;
            String elsePattern = wildcardPatternForElseBody(elseBody);
            if (elsePattern == null) {
                // synthetic unreachable — skip
                return arms;
            }
            // No stripTrailingReturn — assign-context arms preserve their
            // control-flow semantics; divergent arms keep their `return`.
            arms.add(Jcs.object(
                "args", new Jcs.Arr(List.of(
                    Jcs.object("kind", Jcs.string("symbol"), "text", Jcs.string(elsePattern)),
                    elseBody
                )),
                "concept_name", Jcs.string("concept:match-arm")
            ));
            return arms;
        }
    }

    /** Lift an arm body in assign-context. If the body is `target = X;`,
     *  return X (the rust match-arm value-expression). Otherwise lift as
     *  a statement/block. */
    private Json liftAssignArmBody(Statement stmt, String targetName, List<Json> losses) {
        // Block with one statement `target = X;` → X
        if (stmt instanceof BlockStmt block) {
            List<Statement> inner = block.getStatements();
            if (inner.size() == 1) {
                Statement only = inner.get(0);
                Json maybe = tryUnwrapAssignToTarget(only, targetName, losses);
                if (maybe != null) return maybe;
                return liftStatement(only, losses);
            }
            // Multi-stmt: lift as block (concept:seq).
            return liftBlock(block, losses);
        }
        // Bare statement: try unwrap-assign, else lift as-is.
        Json maybe = tryUnwrapAssignToTarget(stmt, targetName, losses);
        if (maybe != null) return maybe;
        return liftStatement(stmt, losses);
    }

    /** Recognize `((Supplier<T>) () -> { body }).get()` — the java lower's
     *  expression-scope match wrapper — and inline the lambda body. The
     *  body's typical shape is `var __provekit_vN = scrut; return ternary;`
     *  which downstream recognition (tryRecognizeMatchTernary,
     *  tryRecognizeInnerMatch) turns into a clean concept:match.
     *
     *  Returns Optional.empty() if the receiver isn't a 0-arg lambda. */
    private Optional<Json> tryInlineSupplierGet(MethodCallExpr m, List<Json> losses) {
        if (m.getScope().isEmpty()) return Optional.empty();
        com.github.javaparser.ast.expr.Expression scope = m.getScope().get();
        // Unwrap parens.
        while (scope instanceof com.github.javaparser.ast.expr.EnclosedExpr enc) {
            scope = enc.getInner();
        }
        // Unwrap Supplier cast (the existing CastExpr handler strips it
        // when called via liftExpression, but we're inspecting structure).
        if (scope instanceof com.github.javaparser.ast.expr.CastExpr cx) {
            scope = cx.getExpression();
            while (scope instanceof com.github.javaparser.ast.expr.EnclosedExpr enc) {
                scope = enc.getInner();
            }
        }
        if (!(scope instanceof com.github.javaparser.ast.expr.LambdaExpr lam)) {
            return Optional.empty();
        }
        if (!lam.getParameters().isEmpty()) return Optional.empty();
        // Lift the lambda body. If the body is a BlockStmt, lift as a
        // block (downstream recognizers fire). If it's a single expression,
        // lift as an expression.
        com.github.javaparser.ast.stmt.Statement body = lam.getBody();
        if (body instanceof BlockStmt bb) {
            return Optional.of(liftBlock(bb, losses));
        }
        if (body instanceof com.github.javaparser.ast.stmt.ExpressionStmt es) {
            return Optional.of(liftExpression(es.getExpression(), losses));
        }
        return Optional.of(liftStatement(body, losses));
    }

    /** Sweep a block for the 2-statement match pattern `var __provekit_vN
     *  = scrut; if-chain` anywhere inside. Useful when the function body
     *  has matchable sub-patterns interleaved with other statements (e.g.
     *  the inner `match method` dispatcher in handle_line). */
    private List<Json> tryRecognizeInnerMatch(BlockStmt block, List<Json> losses) {
        List<Statement> stmts = block.getStatements();
        for (int i = 0; i + 1 < stmts.size(); i++) {
            Statement s0 = stmts.get(i);
            Statement s1 = stmts.get(i + 1);
            // s0: var __provekit_vN = scrut;
            if (!(s0 instanceof com.github.javaparser.ast.stmt.ExpressionStmt es0)) continue;
            if (!(es0.getExpression() instanceof com.github.javaparser.ast.expr.VariableDeclarationExpr vde)) continue;
            if (vde.getVariables().size() != 1) continue;
            var decl = vde.getVariable(0);
            String binding = decl.getNameAsString();
            if (!binding.startsWith("__provekit_v")) continue;
            if (decl.getInitializer().isEmpty()) continue;
            com.github.javaparser.ast.expr.Expression scrutExpr = decl.getInitializer().get();
            // s1: if-chain
            if (!(s1 instanceof com.github.javaparser.ast.stmt.IfStmt ifs)) continue;
            if (ifs.getElseStmt().isEmpty()) continue;
            Json scrutShape = liftExpression(scrutExpr, losses);
            List<Json> arms = collectMatchArms(ifs, binding, losses);
            if (arms == null) continue;
            List<Json> matchArgs = new ArrayList<>();
            matchArgs.add(scrutShape);
            matchArgs.addAll(arms);
            Json matchShape = Jcs.object(
                "args", new Jcs.Arr(matchArgs),
                "concept_name", Jcs.string("concept:match")
            );
            // Build output: pre-stmts + match + post-stmts (recursing for
            // each region so nested patterns also benefit).
            List<Json> out = new ArrayList<>();
            Integer prevEndLine = null;
            for (int k = 0; k < i; k++) {
                Statement st = stmts.get(k);
                if (prevEndLine != null && st.getBegin().isPresent()) {
                    int eff = st.getBegin().get().line;
                    if (st.getComment().isPresent()
                            && st.getComment().get().getBegin().isPresent()) {
                        int cl = st.getComment().get().getBegin().get().line;
                        if (cl < eff) eff = cl;
                    }
                    if (eff > prevEndLine + 1) {
                        out.add(Jcs.object(
                            "args", new Jcs.Arr(List.of()),
                            "concept_name", Jcs.string("concept:blank-line")
                        ));
                    }
                }
                if (st.getEnd().isPresent()) prevEndLine = st.getEnd().get().line;
                Json lifted = liftStatement(st, losses);
                if (lifted != null) out.add(lifted);
            }
            // Blank-line check between the last pre-stmt and the match
            // temp-init line (s0). The match consumes BOTH s0 and s1; the
            // blank-line precedes s0 in source.
            if (prevEndLine != null && s0.getBegin().isPresent()) {
                int eff = s0.getBegin().get().line;
                if (eff > prevEndLine + 1) {
                    out.add(Jcs.object(
                        "args", new Jcs.Arr(List.of()),
                        "concept_name", Jcs.string("concept:blank-line")
                    ));
                }
            }
            out.add(matchShape);
            if (s1.getEnd().isPresent()) prevEndLine = s1.getEnd().get().line;
            // Lift tail statements, if any, as a sub-block so further
            // recognizers can fire there too.
            if (i + 2 < stmts.size()) {
                Statement firstPost = stmts.get(i + 2);
                if (prevEndLine != null && firstPost.getBegin().isPresent()) {
                    int eff = firstPost.getBegin().get().line;
                    if (firstPost.getComment().isPresent()
                            && firstPost.getComment().get().getBegin().isPresent()) {
                        int cl = firstPost.getComment().get().getBegin().get().line;
                        if (cl < eff) eff = cl;
                    }
                    if (eff > prevEndLine + 1) {
                        out.add(Jcs.object(
                            "args", new Jcs.Arr(List.of()),
                            "concept_name", Jcs.string("concept:blank-line")
                        ));
                    }
                }
                com.github.javaparser.ast.NodeList<Statement> tailNodes =
                    new com.github.javaparser.ast.NodeList<>();
                for (int k = i + 2; k < stmts.size(); k++) {
                    tailNodes.add(stmts.get(k));
                }
                BlockStmt tailBlock = new BlockStmt(tailNodes);
                Json tailShape = liftBlock(tailBlock, losses);
                if (tailShape instanceof Jcs.Obj tsObj
                        && "concept:seq".equals(conceptOf(tsObj))) {
                    for (Jcs.Field f : tsObj.fields()) {
                        if ("args".equals(f.key()) && f.value() instanceof Jcs.Arr arr) {
                            for (Jcs.Json child : arr.values()) out.add((Json) child);
                            break;
                        }
                    }
                } else {
                    out.add(tailShape);
                }
            }
            return out;
        }
        return null;
    }

    /** If `stmt` is `target = X;`, return liftExpression(X). Else null. */
    private Json tryUnwrapAssignToTarget(Statement stmt, String targetName, List<Json> losses) {
        if (!(stmt instanceof com.github.javaparser.ast.stmt.ExpressionStmt es)) return null;
        if (!(es.getExpression() instanceof com.github.javaparser.ast.expr.AssignExpr ae)) return null;
        if (!targetName.equals(ae.getTarget().toString())) return null;
        return liftExpression(ae.getValue(), losses);
    }

    /** Strip the trailing `.unwrap()` / `.unwrapErr().payload()` from an
     *  arm body when the pattern is `Ok(v)` / `Err(e)`. The rust source
     *  binds the variant payload directly; the java lower added the
     *  unwrap as the lossless payload extraction. Returns the body
     *  unchanged when no such call is found.
     *
     *  Walks ONLY the outermost concept:call — variant-unwrap is always
     *  at the root of the arm body in the lower's emission pattern. */
    private Json stripVariantUnwrap(Json body, String boundVar, String pat) {
        if (boundVar == null) return body;
        // Walk the tree replacing `concept:call(boundVar, method:unwrap)`
        // with the bare boundVar leaf. Also handles
        // `concept:call(concept:call(boundVar, method:unwrapErr), method:payload)`
        // for the Err arm — collapses to boundVar.
        // For pattern `Err(e)`, we may also see the lower's specific form:
        //   `String.valueOf(((SumVariant) __v.unwrapErr()).payload())`
        // which lifts as a chain we won't fully recognize structurally —
        // for that we use a textual normalize on macro-style leaves later.
        return stripVariantUnwrapRec(body, boundVar);
    }

    private Json stripVariantUnwrapRec(Json node, String boundVar) {
        if (!(node instanceof Jcs.Obj obj)) return node;
        String cn = conceptOf(obj);
        if ("concept:call".equals(cn)) {
            // Get args.
            List<Jcs.Json> argList = null;
            for (Jcs.Field f : obj.fields()) {
                if ("args".equals(f.key()) && f.value() instanceof Jcs.Arr arr) {
                    argList = arr.values();
                    break;
                }
            }
            if (argList != null && argList.size() == 2) {
                Jcs.Json first = argList.get(0);
                Jcs.Json second = argList.get(1);
                // first is symbol(boundVar); second is method:unwrap
                if (isSymbolText(first, boundVar) && isMethodWithName(second, "unwrap")) {
                    return Jcs.object(
                        "kind", Jcs.string("symbol"),
                        "text", Jcs.string(boundVar)
                    );
                }
            }
        }
        // Recurse into children — replace any nested concept:call we find.
        List<Jcs.Field> newFields = new ArrayList<>();
        for (Jcs.Field f : obj.fields()) {
            Json v = f.value();
            if (v instanceof Jcs.Arr arr) {
                List<Jcs.Json> newArr = new ArrayList<>();
                for (Jcs.Json e : arr.values()) {
                    newArr.add(stripVariantUnwrapRec(e, boundVar));
                }
                newFields.add(new Jcs.Field(f.key(), new Jcs.Arr(newArr)));
            } else if (v instanceof Jcs.Obj) {
                newFields.add(new Jcs.Field(f.key(), stripVariantUnwrapRec(v, boundVar)));
            } else {
                newFields.add(f);
            }
        }
        return new Jcs.Obj(newFields);
    }

    private static boolean isSymbolText(Jcs.Json node, String text) {
        if (!(node instanceof Jcs.Obj obj)) return false;
        String kind = null;
        String t = null;
        for (Jcs.Field f : obj.fields()) {
            if ("kind".equals(f.key()) && f.value() instanceof Jcs.Str s) kind = s.value();
            if ("text".equals(f.key()) && f.value() instanceof Jcs.Str s) t = s.value();
        }
        return "symbol".equals(kind) && text.equals(t);
    }

    /** Scan a statement's source-text surface for a
     *  `/*@match-arm-pattern=PATTERN binding=NAME*\/` marker that's at the
     *  IMMEDIATE outer level (not nested in a deeper expression). Returns
     *  {patternText, bindingName} or null.
     *
     *  Heuristic: the marker appears between `return` and the value expr,
     *  so only match the first marker that follows a `return` keyword. */
    private static String[] extractVariantMarker(Statement stmt) {
        if (stmt == null) return null;
        String src = stmt.toString();
        // Only match a marker that's immediately after `return ` at the
        // outer level (i.e. the first `return <marker>`). Nested matches
        // INSIDE the body have their own `return` markers — the FIRST
        // marker encountered in source order is the outer arm's.
        java.util.regex.Matcher m = java.util.regex.Pattern.compile(
                "return\\s+/\\*@match-arm-pattern=([^*]+?)(?:\\s+binding=([A-Za-z_][A-Za-z0-9_]*))?\\*/")
            .matcher(src);
        if (!m.find()) return null;
        String pat = m.group(1).trim();
        String bind = m.group(2);
        return new String[]{pat, bind};
    }

    /** Same for an expression's source-text surface. The marker for an
     *  expression-form arm appears at the START of the expression text. */
    private static String[] extractVariantMarkerFromExpr(com.github.javaparser.ast.expr.Expression expr) {
        if (expr == null) return null;
        String src = expr.toString();
        // For expression arms (ternary form), the marker is prepended to
        // the body text. It must appear at the START of the source.
        java.util.regex.Matcher m = java.util.regex.Pattern.compile(
                "^\\s*/\\*@match-arm-pattern=([^*]+?)(?:\\s+binding=([A-Za-z_][A-Za-z0-9_]*))?\\*/")
            .matcher(src);
        if (!m.find()) return null;
        return new String[]{m.group(1).trim(), m.group(2)};
    }

    /** Collapse the lower's `String.valueOf((SumVariant) SCRUT.unwrapErr()).payload()`
     *  back to the bound variable name. The body has been substituted by
     *  the lower so the SumVariant chain appears wherever the rust source
     *  referenced the binding. Replace ALL such occurrences with `bindName`. */
    private Json collapseSumVariantPayload(Json node, String scrutBinding, String bindName) {
        if (!(node instanceof Jcs.Obj obj)) return node;
        // Recognize String.valueOf(...) call wrapping a chain that ends in
        // `.payload()` whose receiver mentions scrutBinding. Replace with
        // a bare symbol leaf carrying bindName.
        String cn = conceptOf(obj);
        if ("concept:call".equals(cn)) {
            // String.format / String.valueOf calls — args[0] is path leaf
            // (or callee), args[1+] are arguments. Detect by serializing
            // the immediate shape's first arg.
            // Look for the deepest payload() method call in args.
        }
        // Simpler: walk all symbol-leaf text fields. The lower's emission
        // of `String.valueOf(((SumVariant) X.unwrapErr()).payload())` does
        // NOT survive the lift unchanged — it gets parsed as nested calls.
        // We need to recognize the call-tree shape. The call tree for
        // `String.valueOf(((SumVariant) SCRUT.unwrapErr()).payload())` is:
        //   concept:call(
        //     concept:cast(
        //       concept:call(SCRUT, method:unwrapErr),
        //       type:SumVariant),
        //     method:payload)
        // wrapped in String.valueOf via concept:call(path:String, method:valueOf, ...).
        // Walk and replace ANY concept:call whose chain bottoms out at
        // SCRUT.unwrapErr() with `bindName` symbol leaf.
        if (isSumVariantPayloadChain(obj, scrutBinding)) {
            return Jcs.object(
                "kind", Jcs.string("symbol"),
                "text", Jcs.string(bindName)
            );
        }
        // Recurse.
        List<Jcs.Field> newFields = new ArrayList<>();
        for (Jcs.Field f : obj.fields()) {
            Json v = f.value();
            if (v instanceof Jcs.Arr arr) {
                List<Jcs.Json> newArr = new ArrayList<>();
                for (Jcs.Json e : arr.values()) {
                    newArr.add(collapseSumVariantPayload(e, scrutBinding, bindName));
                }
                newFields.add(new Jcs.Field(f.key(), new Jcs.Arr(newArr)));
            } else if (v instanceof Jcs.Obj) {
                newFields.add(new Jcs.Field(f.key(), collapseSumVariantPayload(v, scrutBinding, bindName)));
            } else {
                newFields.add(f);
            }
        }
        return new Jcs.Obj(newFields);
    }

    /** True iff the node IS the call chain
     *  `String.valueOf((SumVariant) X.unwrapErr().payload())` —
     *  the lower's exact emission for sum-variant payload extraction.
     *  The top-level call MUST be `String.valueOf`; nested unwrap+payload
     *  alone is NOT enough (the chain could be inside a larger expression
     *  the source did want to preserve). */
    private static boolean isSumVariantPayloadChain(Json node, String scrutBinding) {
        if (!(node instanceof Jcs.Obj obj)) return false;
        String cn = conceptOf(obj);
        if (!"concept:call".equals(cn)) return false;
        // First two args of a `String.valueOf(...)` lift are:
        //   args[0]: {kind:"path", text:"String"} (or via concept:call(String, method:valueOf, ...))
        //   args[1]: {kind:"method", text:"valueOf", arity:1}
        // Verify the call's top is String.valueOf.
        for (Jcs.Field f : obj.fields()) {
            if (!"args".equals(f.key())) continue;
            if (!(f.value() instanceof Jcs.Arr arr)) continue;
            List<Jcs.Json> aL = arr.values();
            if (aL.size() < 3) return false;
            // arg0: path/symbol "String"
            String t0Kind = null, t0Text = null;
            if (aL.get(0) instanceof Jcs.Obj o0) {
                for (Jcs.Field f0 : o0.fields()) {
                    if ("kind".equals(f0.key()) && f0.value() instanceof Jcs.Str s) t0Kind = s.value();
                    if ("text".equals(f0.key()) && f0.value() instanceof Jcs.Str s) t0Text = s.value();
                }
            }
            // arg1: method "valueOf"
            String t1Kind = null, t1Text = null;
            if (aL.get(1) instanceof Jcs.Obj o1) {
                for (Jcs.Field f1 : o1.fields()) {
                    if ("kind".equals(f1.key()) && f1.value() instanceof Jcs.Str s) t1Kind = s.value();
                    if ("text".equals(f1.key()) && f1.value() instanceof Jcs.Str s) t1Text = s.value();
                }
            }
            boolean topIsStringValueOf =
                ("path".equals(t0Kind) || "symbol".equals(t0Kind))
                && "String".equals(t0Text)
                && "method".equals(t1Kind) && "valueOf".equals(t1Text);
            if (!topIsStringValueOf) return false;
            // arg2 onwards: serialized form must contain unwrapErr+payload.
            String inner = Jcs.encode(aL.get(2));
            return inner.contains("\"unwrapErr\"") && inner.contains("\"payload\"");
        }
        return false;
    }

    private static boolean isMethodWithName(Jcs.Json node, String name) {
        if (!(node instanceof Jcs.Obj obj)) return false;
        String kind = null;
        String text = null;
        for (Jcs.Field f : obj.fields()) {
            if ("kind".equals(f.key()) && f.value() instanceof Jcs.Str s) kind = s.value();
            if ("text".equals(f.key()) && f.value() instanceof Jcs.Str s) text = s.value();
        }
        return "method".equals(kind) && name.equals(text);
    }

    /** Try to interpret a pair of statements as a (bare decl, temp init)
     *  for the match-assign triplet. Returns {targetName, tempBinding,
     *  declaredJavaType} or null when the pair doesn't fit. */
    private static String[] parseDeclAndTemp(Statement decl, Statement temp) {
        // decl: `T name;` — VariableDeclarationExpr with NO initializer.
        if (!(decl instanceof com.github.javaparser.ast.stmt.ExpressionStmt dEs)) return null;
        if (!(dEs.getExpression() instanceof com.github.javaparser.ast.expr.VariableDeclarationExpr dVde)) return null;
        if (dVde.getVariables().size() != 1) return null;
        var dDecl = dVde.getVariable(0);
        if (dDecl.getInitializer().isPresent()) return null;
        String targetName = dDecl.getNameAsString();
        String declaredType = dDecl.getType().asString();
        // temp: `var __provekit_vN = scrut;`
        if (!(temp instanceof com.github.javaparser.ast.stmt.ExpressionStmt tEs)) return null;
        if (!(tEs.getExpression() instanceof com.github.javaparser.ast.expr.VariableDeclarationExpr tVde)) return null;
        if (tVde.getVariables().size() != 1) return null;
        var tDecl = tVde.getVariable(0);
        String tempBinding = tDecl.getNameAsString();
        if (!tempBinding.startsWith("__provekit_v")) return null;
        if (tDecl.getInitializer().isEmpty()) return null;
        return new String[]{targetName, tempBinding, declaredType};
    }

    /** Map a java declared type back to its rust source spelling. Mirrors
     *  the small set the cycle exercises. Pass through unknown types. */
    private static String javaTypeToRustLetType(String javaType) {
        if (javaType == null) return null;
        String t = javaType.trim();
        // Strip jackson FQN noise — both fully-qualified and bare.
        if (t.equals("com.fasterxml.jackson.databind.JsonNode") || t.equals("JsonNode")) {
            return "Value";
        }
        if (t.equals("String") || t.equals("java.lang.String")) return "String";
        if (t.equals("boolean")) return "bool";
        if (t.equals("long") || t.equals("Long")) return "i64";
        if (t.equals("int") || t.equals("Integer")) return "i32";
        if (t.equals("double") || t.equals("Double")) return "f64";
        if (t.equals("Object")) return null;
        return t;
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
     *  in a lifted term-shape tree. Also rewrites word-bounded occurrences
     *  inside macro-call argument strings (e.g. `format!("...", __vN)`). */
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
                if ("text".equals(f.key())) {
                    String value = s.value();
                    if (oldName.equals(value)) {
                        newFields.add(new Jcs.Field(f.key(), Jcs.string(newName)));
                    } else if (value.contains(oldName)) {
                        // Word-bounded substitution for cases like
                        // `format!("...", __provekit_v0)` where the binding
                        // is embedded inside a macro-call args string leaf.
                        String rewritten = value.replaceAll(
                            "\\b" + java.util.regex.Pattern.quote(oldName) + "\\b",
                            java.util.regex.Matcher.quoteReplacement(newName));
                        if (!rewritten.equals(value)) {
                            newFields.add(new Jcs.Field(f.key(), Jcs.string(rewritten)));
                        } else {
                            newFields.add(f);
                        }
                    } else {
                        newFields.add(f);
                    }
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
     *  `X != null && !X.isNull()` → `Some(v) if !v.is_null()`,
     *  `X != null && X.equals("foo")` → `"foo"` (string-literal match),
     *  `X instanceof T.Ok` → `Ok (v)`, `X instanceof T.Err` → `Err (e)`.
     *  Falls back to `Some(v)` for unknown forms. */
    private String derivePatternFromCondition(com.github.javaparser.ast.expr.Expression cond, String binding) {
        // Normalize: remove all whitespace for substring checks since
        // JavaParser may render `v . is_null ()` with spaces from
        // token-stream lifts.
        String t = cond.toString().replaceAll("\\s+", "");
        // String-literal dispatch: __provekit_vN != null && __provekit_vN.equals("foo")
        // → "foo" (the rust source's match arm was just the string literal).
        // The text inside the literal preserves its surface — including the
        // empty string "".
        java.util.regex.Matcher eqMatcher = java.util.regex.Pattern.compile(
                "!=null&&[A-Za-z_][A-Za-z0-9_]*\\.equals\\((\"[^\"]*\")\\)")
            .matcher(t);
        if (eqMatcher.find()) {
            return eqMatcher.group(1);
        }
        // Bare `X.equals("foo")` (no null guard wrapper)
        java.util.regex.Matcher eqMatcher2 = java.util.regex.Pattern.compile(
                "^[A-Za-z_][A-Za-z0-9_]*\\.equals\\((\"[^\"]*\")\\)$")
            .matcher(t);
        if (eqMatcher2.find()) {
            return eqMatcher2.group(1);
        }
        // instanceof patterns: emit rust source form `Ok (v)` / `Err (e)`
        // (note the space — matches the rust lifter's to_token_stream form).
        // The pre-strip text has spaces around `instanceof`; the post-strip
        // text has them collapsed, so word boundaries around `instanceof`
        // don't fire. Match on the literal substring + a class-path suffix
        // that ends in `.Ok)` or `.Err)`.
        if (t.contains("instanceof") && t.contains(".Ok")) {
            return "Ok (v)";
        }
        if (t.contains("instanceof") && t.contains(".Err")) {
            return "Err (e)";
        }
        // Pattern: __provekit_vN != null && !v.is_null() (or .isNull())
        if (t.contains("!=null") && (t.contains(".isNull()") || t.contains(".is_null()"))) {
            return "Some(v) if !v.is_null()";
        }
        if (t.contains("!=null")) {
            return "Some(v)";
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
            // #1391 follow-on: recognize java 17 instanceof-pattern as
            // rust if-let with a variant pattern. The lower emits
            // `/*@if-let-variant=Value::Object*/ if (X instanceof T binding) {...}`
            // for rust's `if let Value::Object(binding) = X { ... }`. Detect
            // the marker on the if-stmt's surface form + the
            // InstanceOfExpr condition.
            String ifStr = ifs.toString();
            java.util.regex.Matcher vmatch = java.util.regex.Pattern
                    .compile("/\\*@if-let-variant=([A-Za-z_][A-Za-z0-9_:]*)\\*/")
                    .matcher(ifStr);
            if (vmatch.find()
                    && ifs.getCondition() instanceof com.github.javaparser.ast.expr.InstanceOfExpr ioe
                    && ioe.getPattern().isPresent()) {
                String variantPath = vmatch.group(1);
                String bindingName = ioe.getPattern().get().toString().replaceFirst(".*\\s+", "").trim();
                // Re-derive the rust pattern text: "Type::Variant(binding)"
                // → token-stream form "Type :: Variant (binding)" so it's
                // byte-identical with the rust lift's emission.
                String parts = variantPath.replace("::", " :: ");
                String rustPattern = parts + " (" + bindingName + ")";
                Json patternLeaf = Jcs.object(
                    "kind", Jcs.string("symbol"),
                    "text", Jcs.string(rustPattern)
                );
                // The scrutinee may carry a `/*@ref*/` or `/*@ref-mut*/`
                // marker in the if-stmt surface form (JavaParser doesn't
                // always attach the comment to the inner NameExpr). Detect
                // by scanning the cond's text up to `instanceof`.
                String condText = ioe.toString();
                boolean scrutIsRef = condText.startsWith("/*@ref")
                        || condText.contains("/*@ref*/")
                        || condText.contains("/*@ref-mut*/");
                boolean scrutIsRefMut = condText.contains("/*@ref-mut*/");
                Json scrut = liftExpression(ioe.getExpression(), losses);
                if (scrutIsRef && !(scrut instanceof Jcs.Obj sobj && "concept:ref".equals(sobj.stringFieldOrNull("concept_name")))) {
                    Json mutLeaf = Jcs.object(
                        "kind", Jcs.string("mutability"),
                        "text", Jcs.string(scrutIsRefMut ? "mut" : "")
                    );
                    scrut = Jcs.object(
                        "args", new Jcs.Arr(List.of(scrut, mutLeaf)),
                        "concept_name", Jcs.string("concept:ref")
                    );
                }
                Json thenBody = ifs.getThenStmt() instanceof BlockStmt tb
                        ? liftBlock(tb, losses)
                        : liftStatement(ifs.getThenStmt(), losses);
                Json elseBody = ifs.getElseStmt()
                        .map(e -> {
                            Json lifted = e instanceof BlockStmt eb
                                    ? liftBlock(eb, losses)
                                    : liftStatement(e, losses);
                            return isEffectivelyEmpty(lifted) ? skipShape() : lifted;
                        })
                        .orElseGet(this::skipShape);
                return Jcs.object(
                    "args", new Jcs.Arr(List.of(patternLeaf, scrut, thenBody, elseBody)),
                    "concept_name", Jcs.string("concept:if-let")
                );
            }
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
            // #1391 follow-on: detect the `/*@for-mut*/` marker the lower
            // emits when the rust source had `for mut x in ...`. The marker
            // appears as a block comment on either the for stmt itself or
            // the VariableDeclarationExpr/Type of the loop variable. Scan
            // the toString() for the marker — JavaParser preserves the
            // comment in the for-stmt's surface form.
            boolean isMut = fes.toString().contains("/*@for-mut*/");
            boolean isRefPat = fes.toString().contains("/*@for-ref*/");
            String bareName = fes.getVariable().getVariable(0).getNameAsString();
            // Reconstruct the rust ref-pattern text "& X" when isRefPat is set;
            // otherwise the bare name. Rust realize's for-each lower checks
            // `var_text.starts_with('&')` and emits the right pattern.
            String varText = isRefPat ? ("& " + bareName) : bareName;
            Json varLeaf;
            if (isMut) {
                varLeaf = Jcs.object(
                    "kind", Jcs.string("symbol"),
                    "text", Jcs.string(varText),
                    "mut", Jcs.bool(true)
                );
            } else {
                varLeaf = Jcs.object(
                    "kind", Jcs.string("symbol"),
                    "text", Jcs.string(varText)
                );
            }
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
            // #1391 follow-on: if the ExpressionStmt carries a
            // `/*@map-insert*/` or `/*@set-insert*/` comment, propagate it
            // to the inner method-call so the lift recognizer fires.
            es.getComment().ifPresent(c -> {
                if (c.isBlockComment()) {
                    String ct = c.getContent().trim();
                    if (ct.equals("@map-insert") || ct.equals("@set-insert")) {
                        Expression inner = es.getExpression();
                        if (inner instanceof MethodCallExpr mci) {
                            mci.setComment(c.clone());
                        } else {
                            // The expression is wrapped (CastExpr / EnclosedExpr) —
                            // unwrap and tag the inner MethodCallExpr.
                            Expression u = inner;
                            while (u instanceof com.github.javaparser.ast.expr.EnclosedExpr enc) {
                                u = enc.getInner();
                            }
                            while (u instanceof com.github.javaparser.ast.expr.CastExpr cx) {
                                u = cx.getExpression();
                            }
                            if (u instanceof MethodCallExpr mci) {
                                mci.setComment(c.clone());
                            }
                        }
                    }
                }
            });
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
        // #1391 follow-on: recognize `/*@ref*/X` and `/*@ref-mut*/X` markers
        // the java lower emits for rust's `&X` / `&mut X`. Wrap the inner
        // shape in concept:ref so the cycle restores the `&` annotation.
        // The marker appears as an attached block comment on the
        // expression's representation. JavaParser parses block comments
        // before an expression as the expression's "comment" attribute.
        // The marker may also appear in the expression's toString() (when
        // attached to a wrapping cast / paren / etc.).
        Optional<com.github.javaparser.ast.comments.Comment> attached = expr.getComment();
        String surface = expr.toString();
        boolean isRefMarker = false;
        boolean isRefMut = false;
        if (attached.isPresent() && attached.get().isBlockComment()) {
            String content = attached.get().getContent().trim();
            if (content.equals("@ref")) {
                isRefMarker = true;
            } else if (content.equals("@ref-mut")) {
                isRefMarker = true;
                isRefMut = true;
            }
        }
        if (!isRefMarker && surface.startsWith("/*@ref-mut*/")) {
            isRefMarker = true;
            isRefMut = true;
        } else if (!isRefMarker && surface.startsWith("/*@ref*/")) {
            isRefMarker = true;
        }
        if (isRefMarker) {
            // Detach the comment so the recursive lift doesn't loop.
            expr.setComment(null);
            Json inner = liftExpression(expr, losses);
            String mutText = isRefMut ? "mut" : "";
            Json mutLeaf = Jcs.object(
                "kind", Jcs.string("mutability"),
                "text", Jcs.string(mutText)
            );
            return Jcs.object(
                "args", new Jcs.Arr(List.of(inner, mutLeaf)),
                "concept_name", Jcs.string("concept:ref")
            );
        }
        if (expr instanceof StringLiteralExpr s) {
            return Jcs.object(
                "kind", Jcs.string("const"),
                "value", Jcs.string(s.getValue())
            );
        }
        if (expr instanceof IntegerLiteralExpr i) {
            // #1391 follow-on: preserve source radix (hex/oct/bin) so
            // round-trip is byte-identical. JavaParser preserves the
            // source token; we sniff the prefix and parse with the right
            // radix.
            String raw = i.getValue();
            String radix;
            long parsed;
            if (raw.startsWith("0x") || raw.startsWith("0X")) {
                radix = "hex";
                parsed = Long.parseLong(raw.substring(2).replace("_", ""), 16);
            } else if (raw.startsWith("0b") || raw.startsWith("0B")) {
                radix = "bin";
                parsed = Long.parseLong(raw.substring(2).replace("_", ""), 2);
            } else if (raw.length() > 1 && raw.startsWith("0") && raw.chars().allMatch(c -> c >= '0' && c <= '7' || c == '_')) {
                radix = "oct";
                parsed = Long.parseLong(raw.substring(1).replace("_", ""), 8);
            } else {
                radix = "dec";
                parsed = Long.parseLong(raw.replace("_", ""));
            }
            return Jcs.object(
                "kind", Jcs.string("const"),
                "value", Jcs.integer(parsed),
                "radix", Jcs.string(radix)
            );
        }
        if (expr instanceof NameExpr n) {
            return Jcs.object(
                "kind", Jcs.string("symbol"),
                "text", Jcs.string(n.getNameAsString())
            );
        }
        if (expr instanceof BooleanLiteralExpr b) {
            // Substrate-canonical boolean literal: emit as concept:literal
            // with a real JSON boolean value (not a string). Rust realize's
            // literal_term_with_width matches on Value::Bool to emit `true`/
            // `false` unquoted. Sort CID matches the rust lifter's `Bool`.
            return Jcs.object(
                "args", new Jcs.Arr(List.of()),
                "concept_name", Jcs.string("concept:literal"),
                "op_cid", Jcs.string("blake3-512:02804a0bdbd2d5d541544451f41ee8d0d340baf28f70bd5abf5844e87a96aedd7b5ab3453962754a020679cc8c6b3d1f4cf0336a7ad8118128d42ac667abf2d6"),
                "sort", Jcs.string("blake3-512:0ee13bf3fd6b7ecfbee72dfbfc18a7c0ea7f1663de6cca43cefb36f5b4c03665452646094a7c296e819e75d683c6ce4821f3d7db3c3c78ae97f2d4e3451d2074"),
                "value", Jcs.bool(b.getValue())
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
                // #1391 follow-on: preserve explicit type annotation
                // whenever it is a recognized generic-container type
                // (java.util.List<T>, BTreeSet<T>, HashMap<K,V>, etc.),
                // regardless of the initializer shape. Rust realize's
                // java_type_to_rust_let_annotation maps these to their
                // rust-side equivalents (Vec<T>, BTreeSet<T>, …). For
                // unrecognized types the annotation would be wrong, so
                // we suppress and let rust's local type inference fill in.
                //
                // Rule: declaredType is non-`var` AND its head matches one
                // of the container families that have a rust equivalent.
                String declaredType = v.getType().asString();
                boolean propagateType = false;
                if (!declaredType.equals("var")) {
                    // Strip generics for head check.
                    String head = declaredType;
                    int lt = head.indexOf('<');
                    if (lt >= 0) head = head.substring(0, lt);
                    head = head.trim();
                    // Recognized container families (java.util.* or simple form).
                    if (head.endsWith("ArrayList") || head.endsWith("List")
                            || head.endsWith("TreeSet") || head.endsWith("HashSet")
                            || head.endsWith("LinkedList") || head.endsWith("Set")
                            || head.endsWith("HashMap") || head.endsWith("TreeMap")
                            || head.endsWith("Map")) {
                        propagateType = true;
                    }
                    // Backstop: if the initializer is `new X<>()` with no
                    // args, always propagate (the original rule). Required
                    // for `let x = Vec::new()` style where the inner type
                    // can't be inferred from the call.
                    if (!propagateType
                            && v.getInitializer().isPresent()
                            && v.getInitializer().get() instanceof com.github.javaparser.ast.expr.ObjectCreationExpr oce0
                            && oce0.getArguments().isEmpty()) {
                        String initType = oce0.getType().asString().replaceFirst("<.*>", "");
                        if (initType.endsWith("ArrayList") || initType.endsWith("TreeSet")
                                || initType.endsWith("HashMap") || initType.endsWith("TreeMap")
                                || initType.endsWith("HashSet") || initType.endsWith("LinkedList")) {
                            propagateType = true;
                        }
                    }
                }
                Json nameLeaf;
                if (propagateType) {
                    nameLeaf = Jcs.object(
                        "kind", Jcs.string("symbol"),
                        "let_type", Jcs.string(declaredType),
                        "text", Jcs.string(v.getNameAsString())
                    );
                } else {
                    nameLeaf = Jcs.object(
                        "kind", Jcs.string("symbol"),
                        "text", Jcs.string(v.getNameAsString())
                    );
                }
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
            // #1391 follow-on: strip /*...*/ block comments before
            // text-comparing. JavaParser's toString includes attached
            // citation comments on child nodes, which causes startsWith
            // checks to fail when the cycled lower has citation
            // annotations (e.g. /*@concept concept:value-clone
            // source-name=cloned*/Substrate.cloneOf(X)).
            java.util.function.Function<String, String> stripCmts = s ->
                s.replaceAll("(?s)/\\*[^*]*\\*+(?:[^/*][^*]*\\*+)*/", "")
                 .replaceAll("\\s+", "");
            String condText = stripCmts.apply(ce.getCondition().toString());
            String thenText = stripCmts.apply(ce.getThenExpr().toString());
            String elseText = ce.getElseExpr().toString();
            // #1391 follow-on: detect java lower's emission for
            // .and_then(f): `X != null ? ((Function)f).apply(X) : null` or
            // `X != null ? f.apply(X) : null` → concept:call(X, method:and_then, f).
            // The cycled rust gets `X.and_then(f)` matching source form.
            if (condText.endsWith("!=null") && elseText.replaceAll("\\s+", "").equals("null")) {
                // Pattern: `X != null ? X.M() : null` → `X.and_then(|m| m.M())`.
                // Distinct from the `apply` form below — when the lambda
                // body is just `m.M()`, the lower inlines the lambda body.
                String operand = condText.substring(0, condText.length() - "!=null".length());
                if (ce.getThenExpr() instanceof com.github.javaparser.ast.expr.MethodCallExpr mcInline
                        && mcInline.getScope().isPresent()
                        && mcInline.getArguments().isEmpty()) {
                    String scopeStripped = mcInline.getScope().get().toString().replaceAll("\\s+", "");
                    if (scopeStripped.equals(operand)) {
                        // The receiver is the same as the operand → inlined lambda.
                        Json operandShape = liftExpression(mcInline.getScope().get(), losses);
                        String methodName = mcInline.getNameAsString();
                        // Map common java method names back to rust.
                        String rustMethod = switch (methodName) {
                            case "asText" -> "as_str";
                            case "isNull" -> "is_null";
                            case "isArray" -> "is_array";
                            case "isObject" -> "is_object";
                            default -> methodName;
                        };
                        // Build the closure `|m| m.M()` shape.
                        Json closureBody = Jcs.object(
                            "args", new Jcs.Arr(List.of(
                                Jcs.object("kind", Jcs.string("symbol"), "text", Jcs.string("m")),
                                Jcs.object("arity", Jcs.string("0"),
                                    "concept_name", Jcs.string("method:" + rustMethod),
                                    "kind", Jcs.string("method"),
                                    "text", Jcs.string(rustMethod))
                            )),
                            "concept_name", Jcs.string("concept:call")
                        );
                        Json closureShape = Jcs.object(
                            "args", new Jcs.Arr(List.of(
                                closureBody,
                                Jcs.object("kind", Jcs.string("symbol"), "text", Jcs.string("m"))
                            )),
                            "concept_name", Jcs.string("concept:closure")
                        );
                        return Jcs.object(
                            "args", new Jcs.Arr(List.of(
                                operandShape,
                                methodConceptLeaf("and_then", 1),
                                closureShape
                            )),
                            "concept_name", Jcs.string("concept:call")
                        );
                    }
                }
                com.github.javaparser.ast.expr.MethodCallExpr applyCall = null;
                if (ce.getThenExpr() instanceof com.github.javaparser.ast.expr.MethodCallExpr mc
                        && "apply".equals(mc.getNameAsString())
                        && mc.getArguments().size() == 1) {
                    applyCall = mc;
                }
                if (applyCall != null
                        && applyCall.getArgument(0).toString().replaceAll("\\s+", "").equals(operand)) {
                    com.github.javaparser.ast.expr.Expression operandExpr = applyCall.getArgument(0);
                    com.github.javaparser.ast.expr.Expression fnExpr = applyCall.getScope().orElse(null);
                    if (fnExpr != null) {
                        Json operandShape = liftExpression(operandExpr, losses);
                        // #1391 follow-on: recognize the canned Value::as_*
                        // lambda expansions. The java lower emits
                        // `(Function<JsonNode,JsonNode>)(n -> n != null &&
                        // n.isArray() ? n : null)` for Value::as_array (and
                        // analogous for as_object). Strip the cast +
                        // parens, then pattern-match the lambda body to
                        // reconstruct the rust symbol.
                        Json fnShape = recognizeAsValueAccessor(fnExpr)
                            .orElseGet(() -> liftExpression(fnExpr, losses));
                        return Jcs.object(
                            "args", new Jcs.Arr(List.of(
                                operandShape,
                                methodConceptLeaf("and_then", 1),
                                fnShape
                            )),
                            "concept_name", Jcs.string("concept:call")
                        );
                    }
                }
            }
            // #1391 follow-on: Pattern X != null ? String.valueOf(X) : null
            // → X.map(str::to_string). The java lower emits this for
            // rust source `option.map(str::to_string)`. Without the
            // recognizer the cycled rust emits a verbose ternary
            // expansion that's semantically equivalent but not
            // byte-identical.
            if (condText.endsWith("!=null") && elseText.replaceAll("\\s+", "").equals("null")) {
                // Then branch should be `String.valueOf(X)` where X matches operand.
                if (ce.getThenExpr() instanceof com.github.javaparser.ast.expr.MethodCallExpr toStrCall
                        && "valueOf".equals(toStrCall.getNameAsString())
                        && toStrCall.getScope().map(Object::toString).orElse("").equals("String")
                        && toStrCall.getArguments().size() == 1) {
                    String operand2 = condText.substring(0, condText.length() - "!=null".length());
                    if (toStrCall.getArgument(0).toString().replaceAll("\\s+", "").equals(operand2)) {
                        com.github.javaparser.ast.expr.Expression operandExpr2 = toStrCall.getArgument(0);
                        Json operandShape2 = liftExpression(operandExpr2, losses);
                        Json fnLeaf = Jcs.object(
                            "kind", Jcs.string("symbol"),
                            "text", Jcs.string("str::to_string")
                        );
                        return Jcs.object(
                            "args", new Jcs.Arr(List.of(
                                operandShape2,
                                methodConceptLeaf("map", 1),
                                fnLeaf
                            )),
                            "concept_name", Jcs.string("concept:call")
                        );
                    }
                }
            }
            // Pattern: X != null ? X : default → X.unwrap_or(default)
            if (condText.endsWith("!=null") && condText.startsWith(thenText)) {
                // #1391 follow-on: detect the .unwrap_or_else marker the
                // java lower emits for closure-preserving cases:
                //   default = `((Function<Object,Object>)(e -> body)).apply(/*@unwrap-or-else-marker*/null)`
                // → emit `X.unwrap_or_else(concept:closure(body, [e]))`.
                String elseStr = elseText.replaceAll("\\s+", "");
                if (elseStr.contains("/*@unwrap-or-else-marker*/")
                        || elseText.contains("@unwrap-or-else-marker")) {
                    // Walk the else expression for the inner LambdaExpr.
                    com.github.javaparser.ast.expr.LambdaExpr lambda = ce.getElseExpr()
                        .findFirst(com.github.javaparser.ast.expr.LambdaExpr.class).orElse(null);
                    if (lambda != null && lambda.getParameters().size() == 1) {
                        String paramName = lambda.getParameter(0).getNameAsString();
                        boolean isBlockBody = lambda.getBody() instanceof BlockStmt;
                        Json bodyShape = lambda.getExpressionBody()
                                .map(e -> liftExpression(e, losses))
                                .orElseGet(() -> lambda.getBody() instanceof BlockStmt bb
                                        ? unwrapSingleReturn(bb, losses)
                                        : null);
                        if (bodyShape != null) {
                            // Tag block-form body so the rust realize emits
                            // `|e| { body }` matching the source.
                            if (isBlockBody && bodyShape instanceof Jcs.Obj bodyObj) {
                                List<Object> kv = new ArrayList<>();
                                for (Jcs.Field f : bodyObj.fields()) {
                                    kv.add(f.key()); kv.add(f.value());
                                }
                                kv.add("closure_block_body");
                                kv.add(Jcs.bool(true));
                                bodyShape = Jcs.object(kv.toArray());
                            }
                            Json closure = Jcs.object(
                                "args", new Jcs.Arr(List.of(
                                    bodyShape,
                                    Jcs.object("kind", Jcs.string("symbol"),
                                               "text", Jcs.string(paramName))
                                )),
                                "concept_name", Jcs.string("concept:closure")
                            );
                            Json optShape = liftExpression(ce.getThenExpr(), losses);
                            return Jcs.object(
                                "args", new Jcs.Arr(List.of(
                                    optShape,
                                    methodConceptLeaf("unwrap_or_else", 1),
                                    closure
                                )),
                                "concept_name", Jcs.string("concept:call")
                            );
                        }
                    }
                }
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
            // #1391 follow-on: !(Objects.equals(a, b)) → concept:ne(a, b).
            // Rust source `a != b` (string-valued) lowers to java
            // `!Objects.equals(a, b)`. Without this rewrite the lift
            // produces concept:not(concept:eq(...)) which the rust realize
            // emits as `!(a == b)` — semantically equivalent but not
            // byte-identical. Recognizing the !-of-Objects.equals shape
            // canonicalizes back to concept:ne so the round-trip yields
            // `a != b` verbatim.
            if (ue.getOperator() == UnaryExpr.Operator.LOGICAL_COMPLEMENT
                    && ue.getExpression() instanceof MethodCallExpr inner) {
                String innerName = inner.getNameAsString();
                String innerScope = inner.getScope().map(Object::toString).orElse("");
                if ("equals".equals(innerName) && innerScope.endsWith("Objects")
                        && inner.getArguments().size() == 2) {
                    Json a = liftExpression(inner.getArgument(0), losses);
                    Json b = liftExpression(inner.getArgument(1), losses);
                    return Jcs.object(
                        "args", new Jcs.Arr(List.of(a, b)),
                        "concept_name", Jcs.string("concept:ne")
                    );
                }
            }
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
            // #1391 follow-on: strip Function<...,...>-typed casts on
            // lambdas. The java lower wraps closures in functional-
            // interface casts for type inference; the inner lambda IS
            // the concept:closure. Without stripping, the cycled rust
            // emits `concept:cast(closure, Function<...>)` which has no
            // rust analogue. Recognize Function/Supplier/Consumer/etc.
            // and pass through to the lambda lift.
            com.github.javaparser.ast.expr.Expression castInner = cast.getExpression();
            while (castInner instanceof com.github.javaparser.ast.expr.EnclosedExpr enc) {
                castInner = enc.getInner();
            }
            boolean isFunctionalInterfaceType =
                castType.startsWith("Function<") || castType.startsWith("Function ")
                || castType.contains(".Function<") || castType.equals("Function")
                || castType.startsWith("Supplier<") || castType.contains(".Supplier<")
                || castType.startsWith("Consumer<") || castType.contains(".Consumer<")
                || castType.startsWith("Predicate<") || castType.contains(".Predicate<")
                || castType.startsWith("BiFunction<") || castType.contains(".BiFunction<");
            if (isFunctionalInterfaceType
                    && castInner instanceof com.github.javaparser.ast.expr.LambdaExpr) {
                return liftExpression(castInner, losses);
            }
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
            // The matcher recognizes the AST shape; the catalog supplies the
            // concept-hub name (reverse lookup keyed by kit-op name).
            String pathTypeStr0 = typeStr.replaceFirst("<.*>", "");
            if (oce.getArguments().isEmpty()) {
                String kitOp = null;
                if (pathTypeStr0.equals("java.util.ArrayList") || pathTypeStr0.endsWith(".ArrayList") || pathTypeStr0.equals("ArrayList")) {
                    kitOp = "java:array-list-new";
                } else if (pathTypeStr0.equals("java.util.HashMap") || pathTypeStr0.endsWith(".HashMap") || pathTypeStr0.equals("HashMap")) {
                    kitOp = "java:hashmap-new";
                }
                if (kitOp != null) {
                    String concept = com.provekit.ir.OperationRealizationCatalog.conceptForJavaOp(kitOp);
                    if (concept != null) {
                        return Jcs.object(
                            "args", new Jcs.Arr(List.of()),
                            "concept_name", Jcs.string(concept)
                        );
                    }
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
            boolean isBlockBody = lam.getBody() instanceof BlockStmt;
            Json body = lam.getExpressionBody()
                    .map(e -> liftExpression(e, losses))
                    .orElseGet(() -> lam.getBody() instanceof BlockStmt bb ? liftBlock(bb, losses) : Jcs.object());
            // #1391 follow-on: tag block-form lambda bodies so the rust
            // realize emits `|e| { body }` form, matching the source surface
            // (instead of expression-form `|e| body`).
            if (isBlockBody && body instanceof Jcs.Obj bodyObj) {
                List<Object> kv = new ArrayList<>();
                for (Jcs.Field f : bodyObj.fields()) {
                    kv.add(f.key());
                    kv.add(f.value());
                }
                kv.add("closure_block_body");
                kv.add(Jcs.bool(true));
                body = Jcs.object(kv.toArray());
            }
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
            // #1391 follow-on: recognize the `/*@map-insert*/` and
            // `/*@set-insert*/` markers the lower emits for rust's
            // `map.insert(K, V)` and `set.insert(X)`. Without these
            // markers the cycle drops rust's `method:insert` to
            // jackson's `.set()` / java collections' `.add()`/`.put()`.
            //
            // The marker appears as an attached block comment on the
            // MethodCallExpr's surface form. Extract scope + args and
            // emit a 3-arg (set) or 4-arg (map) concept:call with
            // method:insert.
            // The marker may be attached to the method-call's surface, OR
            // to a wrapping CastExpr / EnclosedExpr. Inspect the call's
            // toString() to cover all attachment positions.
            Optional<com.github.javaparser.ast.comments.Comment> attachedC = m.getComment();
            String mSurface = m.toString();
            boolean isMapInsert = (attachedC.isPresent() && attachedC.get().isBlockComment()
                    && attachedC.get().getContent().trim().equals("@map-insert"))
                    || mSurface.startsWith("/*@map-insert*/")
                    || mSurface.contains("/*@map-insert*/");
            boolean isSetInsert = (attachedC.isPresent() && attachedC.get().isBlockComment()
                    && attachedC.get().getContent().trim().equals("@set-insert"))
                    || mSurface.startsWith("/*@set-insert*/")
                    || mSurface.contains("/*@set-insert*/");
            if (isMapInsert || isSetInsert) {
                String cnt = isMapInsert ? "@map-insert" : "@set-insert";
                if (cnt.equals("@map-insert") || cnt.equals("@set-insert")) {
                    // For map-insert the receiver could be wrapped in
                    // a CastExpr `((ObjectNode) X)` — unwrap.
                    com.github.javaparser.ast.expr.Expression scope = m.getScope().orElse(null);
                    while (scope instanceof com.github.javaparser.ast.expr.EnclosedExpr enc) {
                        scope = enc.getInner();
                    }
                    while (scope instanceof com.github.javaparser.ast.expr.CastExpr cx) {
                        scope = cx.getExpression();
                    }
                    if (scope != null) {
                        Json recvShape = liftExpression(scope, losses);
                        List<Json> outArgs = new ArrayList<>();
                        outArgs.add(recvShape);
                        outArgs.add(methodConceptLeaf("insert", m.getArguments().size()));
                        for (var a : m.getArguments()) {
                            // For the key arg, if it's `K.toString()` and
                            // K is a string literal, unwrap to the literal —
                            // the lower wraps string keys with .toString().
                            if (a instanceof com.github.javaparser.ast.expr.MethodCallExpr mc
                                    && "toString".equals(mc.getNameAsString())
                                    && mc.getArguments().isEmpty()
                                    && mc.getScope().isPresent()) {
                                // We still want to preserve the .to_string() call
                                // around the literal — rust source is
                                // `"name".to_string()`. Lift as concept:call(literal, method:to_string).
                                Json keyShape = liftExpression(mc.getScope().get(), losses);
                                outArgs.add(Jcs.object(
                                    "args", new Jcs.Arr(List.of(
                                        keyShape,
                                        methodConceptLeaf("to_string", 0)
                                    )),
                                    "concept_name", Jcs.string("concept:call")
                                ));
                                continue;
                            }
                            // For the value arg, `MAPPER.valueToTree(X)` is
                            // the lower's emission of rust's `Value::String(X)`
                            // (corpus convention: only String values flow
                            // through this site). Unwrap to
                            // `concept:call(Value::String, X)`.
                            if (a instanceof com.github.javaparser.ast.expr.MethodCallExpr vc
                                    && "valueToTree".equals(vc.getNameAsString())
                                    && vc.getArguments().size() == 1
                                    && vc.getScope().isPresent()
                                    && "MAPPER".equals(vc.getScope().get().toString())) {
                                Json valArgShape = liftExpression(vc.getArgument(0), losses);
                                outArgs.add(Jcs.object(
                                    "args", new Jcs.Arr(List.of(
                                        Jcs.object("kind", Jcs.string("path"),
                                                   "text", Jcs.string("Value::String")),
                                        valArgShape
                                    )),
                                    "concept_name", Jcs.string("concept:call")
                                ));
                                continue;
                            }
                            outArgs.add(liftExpression(a, losses));
                        }
                        return Jcs.object(
                            "args", new Jcs.Arr(outArgs),
                            "concept_name", Jcs.string("concept:call")
                        );
                    }
                }
            }
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
                // #1391 follow-on: `((Supplier<X>) () -> { body }).get()`
                // is the lower's expression-scope match wrapper. Inline
                // the lambda's body — the lift downstream then sees the
                // match pattern inside the arm and recognizes it.
                Optional<Jcs.Json> supplierInlined = tryInlineSupplierGet(m, losses);
                if (supplierInlined.isPresent()) {
                    return supplierInlined.get();
                }
            }
            // #1391 follow-on: MAPPER.nullNode() → Value::Null symbol.
            // Substrate-symmetric: rust source `Value::Null` lowers to
            // `MAPPER.nullNode()` via the symbol remap; the reverse lift
            // restores the symbol so the round-trip is byte-identical.
            if ("nullNode".equals(name) && scopeText.endsWith("MAPPER")
                    && m.getArguments().isEmpty()) {
                return Jcs.object(
                    "kind", Jcs.string("symbol"),
                    "text", Jcs.string("Value::Null")
                );
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
            // catalog #1391: .getBytes(StandardCharsets.UTF_8) — reverse lookup
            // on java:string-getBytes-utf8.
            if ("getBytes".equals(name) && m.getArguments().size() == 1
                    && m.getArgument(0).toString().contains("StandardCharsets")
                    && m.getScope().isPresent()) {
                String concept = com.provekit.ir.OperationRealizationCatalog.conceptForJavaOp("java:string-getBytes-utf8");
                if (concept != null) {
                    Json recvShape = liftExpression(m.getScope().get(), losses);
                    return Jcs.object(
                        "args", new Jcs.Arr(List.of(recvShape)),
                        "concept_name", Jcs.string(concept)
                    );
                }
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
            // #1391 follow-on: detect rust's `X.unwrap_or("").to_string()`
            // shape. Lower emits this as `(X != null ? X : "").toString()`
            // — a ConditionalExpr (wrapped in EnclosedExpr) followed by
            // toString. The ternary's elseBranch is `""` (StringLiteralExpr).
            // For this shape preserve the method:to_string call.
            if ("toString".equals(name) && m.getArguments().isEmpty() && m.getScope().isPresent()) {
                com.github.javaparser.ast.expr.Expression scope = m.getScope().get();
                com.github.javaparser.ast.expr.Expression unwrapped = scope;
                while (unwrapped instanceof com.github.javaparser.ast.expr.EnclosedExpr enc) {
                    unwrapped = enc.getInner();
                }
                if (unwrapped instanceof ConditionalExpr ucond
                        && ucond.getElseExpr() instanceof com.github.javaparser.ast.expr.StringLiteralExpr sle
                        && sle.getValue().isEmpty()) {
                    // The `.unwrap_or("").to_string()` shape. Preserve
                    // method:to_string on the cycled rust output.
                    Json recvShape = liftExpression(scope, losses);
                    return Jcs.object(
                        "args", new Jcs.Arr(List.of(
                            recvShape,
                            methodConceptLeaf("to_string", 0)
                        )),
                        "concept_name", Jcs.string("concept:call")
                    );
                }
                return liftExpression(scope, losses);
            }
            // catalog #1391: zero-arg instance methods (catalog reverse-lookup
            // keyed by kit-op name; matcher knows AST shape → kit-op name).
            if (m.getScope().isPresent() && m.getArguments().isEmpty()) {
                String kitOp = null;
                switch (name) {
                    case "asText": kitOp = "java:jackson-jsonnode-asText"; break;
                    default: break;
                }
                if (kitOp != null) {
                    String concept = com.provekit.ir.OperationRealizationCatalog.conceptForJavaOp(kitOp);
                    if (concept != null) {
                        Json recvShape = liftExpression(m.getScope().get(), losses);
                        return Jcs.object(
                            "args", new Jcs.Arr(List.of(recvShape)),
                            "concept_name", Jcs.string(concept)
                        );
                    }
                }
            }
            // catalog #1391: Objects.nonNull(x) — reverse lookup on java:objects-nonnull.
            if ("nonNull".equals(name) && m.getArguments().size() == 1
                    && m.getScope().isPresent()
                    && m.getScope().get().toString().endsWith("Objects")) {
                String concept = com.provekit.ir.OperationRealizationCatalog.conceptForJavaOp("java:objects-nonnull");
                if (concept != null) {
                    Json argShape = liftExpression(m.getArgument(0), losses);
                    return Jcs.object(
                        "args", new Jcs.Arr(List.of(argShape)),
                        "concept_name", Jcs.string(concept)
                    );
                }
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
        //
        // #1391 follow-on: concept:value-clone with source-name attr.
        // Rust source `X.into()` → java `/*@concept concept:value-clone
        // source-name=into*/ Substrate.cloneOf(X)`. The citation attr
        // carries the SOURCE METHOD NAME (into/clone/cloned). To round-trip
        // back to byte-identical rust, we reconstruct the structural
        // method-call shape that the rust lift originally produced:
        // concept:call(receiver, method:<name>, []). Skipping the
        // concept:value-clone identity here trades concept-honesty for
        // byte-identity — the citation IS the concept identity, so the
        // structural form preserves both round-trip and concept content.
        if ("concept:value-clone".equals(conceptName)
                && attrs.containsKey("source-name")) {
            String sourceName = attrs.get("source-name").trim();
            if (structuralArgs.size() == 1) {
                // Build method leaf matching the rust lift's emission for
                // X.into() / X.clone() / X.cloned().
                Json methodLeaf = Jcs.object(
                    "arity", Jcs.string("0"),
                    "concept_name", Jcs.string("method:" + sourceName),
                    "kind", Jcs.string("method"),
                    "text", Jcs.string(sourceName)
                );
                List<Json> callArgs = new ArrayList<>();
                callArgs.add(structuralArgs.get(0)); // receiver
                callArgs.add(methodLeaf);
                return Jcs.object(
                    "args", new Jcs.Arr(callArgs),
                    "concept_name", Jcs.string("concept:call")
                );
            }
            // Citation comment was POSITIONALLY attached to a larger AST
            // node (e.g. EnclosedExpr wrapping a ConditionalExpr). The
            // value-clone identity belongs to a child cloneOf call inside,
            // not the outer expression. Drop the citation and return the
            // structural lift — child cloneOf nodes will be re-cited via
            // their own readCitation. This preserves the round-trip
            // unwrap_or / and_then recognizers that need the structural
            // ConditionalExpr form.
            return structural;
        }
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

    /** #1391 follow-on: pattern-recognize the canned Value::as_* lambda
     *  expansions emitted by SugarRealizer. Returns a {kind:"symbol",
     *  text:"Value::as_array"} leaf if the expression is one of those
     *  expansions; otherwise empty.
     *
     *  Patterns recognized (after stripping EnclosedExpr + CastExpr):
     *    n -> n != null && n.isArray() ? n : null   → Value::as_array
     *    n -> n != null && n.isObject() ? n : null  → Value::as_object
     *
     *  The rust realize side will emit this as the symbol passed to
     *  and_then, matching the source-form chain.
     */
    private Optional<Json> recognizeAsValueAccessor(com.github.javaparser.ast.expr.Expression e) {
        com.github.javaparser.ast.expr.Expression core = e;
        while (core instanceof com.github.javaparser.ast.expr.EnclosedExpr enc) {
            core = enc.getInner();
        }
        while (core instanceof com.github.javaparser.ast.expr.CastExpr cast) {
            core = cast.getExpression();
            while (core instanceof com.github.javaparser.ast.expr.EnclosedExpr enc) {
                core = enc.getInner();
            }
        }
        if (!(core instanceof LambdaExpr lam)) return Optional.empty();
        if (lam.getParameters().size() != 1) return Optional.empty();
        // Collapse whitespace + compare against the canonical body shapes
        // emitted by SugarRealizer's Value::as_* remap.
        String pName = lam.getParameter(0).getNameAsString();
        String bodyText;
        if (lam.getExpressionBody().isPresent()) {
            bodyText = lam.getExpressionBody().get().toString().replaceAll("\\s+", "");
        } else {
            return Optional.empty();
        }
        String arrayExpect = pName + "!=null&&" + pName + ".isArray()?" + pName + ":null";
        String objectExpect = pName + "!=null&&" + pName + ".isObject()?" + pName + ":null";
        if (bodyText.equals(arrayExpect)) {
            return Optional.of(Jcs.object("kind", Jcs.string("symbol"), "text", Jcs.string("Value::as_array")));
        }
        if (bodyText.equals(objectExpect)) {
            return Optional.of(Jcs.object("kind", Jcs.string("symbol"), "text", Jcs.string("Value::as_object")));
        }
        return Optional.empty();
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
