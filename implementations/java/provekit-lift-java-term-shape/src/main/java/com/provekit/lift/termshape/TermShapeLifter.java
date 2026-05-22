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

        return new LiftedMethod(shape, paramNames, paramTypes, returnType, losses);
    }

    /** Lift a block as concept:seq of its statements. */
    private Json liftBlock(BlockStmt block, List<Json> losses) {
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
            Json elseBranch = ifs.getElseStmt()
                    .map(e -> e instanceof BlockStmt eb ? liftBlock(eb, losses) : liftStatement(e, losses))
                    .orElseGet(() -> Jcs.object(
                        "args", new Jcs.Arr(List.of()),
                        "concept_name", Jcs.string("concept:skip")
                    ));
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
            return Jcs.object(
                "kind", Jcs.string("const"),
                "value", Jcs.string(i.getValue())
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
            // Ternary `cond ? then : else` → concept:conditional.
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
            return Jcs.object(
                "args", new Jcs.Arr(List.of(
                    liftExpression(cast.getExpression(), losses),
                    Jcs.object(
                        "kind", Jcs.string("type"),
                        "text", Jcs.string(cast.getType().asString())
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
            // Default: `new Type(args)` → concept:call with ::new path leaf.
            List<Json> args = new ArrayList<>();
            args.add(Jcs.object(
                "kind", Jcs.string("path"),
                "text", Jcs.string(typeStr + "::new")
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
            // `Type::method` — emit as path-leaf (the canonical form for
            // function references in our ProofIR).
            return Jcs.object(
                "kind", Jcs.string("path"),
                "text", Jcs.string(mre.getScope().toString() + "::" + mre.getIdentifier())
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
                    return Jcs.object(
                        "args", new Jcs.Arr(List.of(liftExpression(m.getArgument(0), losses))),
                        "concept_name", Jcs.string("concept:try-unwrap")
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
            // Default: concept:call(receiver, method-concept-leaf, args...).
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
