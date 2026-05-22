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
        if (expr instanceof MethodCallExpr m) {
            List<Json> args = new ArrayList<>();
            m.getScope().ifPresent(scope -> args.add(liftExpression(scope, losses)));
            // Method-concept leaf — canonical content-addressed identity
            // (structure determines CID).
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
        // Minimal: extract the concept name (first token after @concept).
        String body = citation.substring("@concept".length()).trim();
        int firstSpace = body.indexOf(' ');
        String conceptName = firstSpace < 0 ? body : body.substring(0, firstSpace).trim();
        // For now, citation reconstruction is partial — we record what
        // we got and acknowledge the structural-faithful path is TBD.
        recordLoss(losses, "citation-partial-reconstruct",
                "concept=" + conceptName + " at " + nodeLocation(sourceNode));
        return Jcs.object(
            "args", new Jcs.Arr(List.of()),
            "concept_name", Jcs.string(conceptName)
        );
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
