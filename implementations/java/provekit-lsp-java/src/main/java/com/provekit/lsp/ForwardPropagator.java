package com.provekit.lsp;

/**
 * ForwardPropagator — accumulate posts and emit implication-check diagnostics.
 *
 * Per: docs/lsp/forward-propagation-floor-v1.md
 *
 * IN scope at v1.0.0 floor:
 * - Variable assignment posts
 * - Sequential flow
 * - If/else branch merge (G3 disjunction)
 * - Function call posts from seed catalog
 * - Callsite pre-check (implication query)
 * - top fallback for out-of-scope constructs
 */
public class ForwardPropagator {
    private final java.util.Map<String, Post> seedCatalog = new java.util.HashMap<>();

    public static class Post {
        public final java.util.List<String> constraints;
        public final boolean isTop;

        public Post(java.util.List<String> constraints, boolean isTop) {
            this.constraints = constraints;
            this.isTop = isTop;
        }

        public static Post top() {
            return new Post(java.util.Collections.emptyList(), true);
        }

        public static Post of(String constraint) {
            java.util.List<String> c = new java.util.ArrayList<>();
            c.add(constraint);
            return new Post(c, false);
        }
    }

    public void addToCatalog(String calleeId, Post pre, Post post) {
        seedCatalog.put(calleeId, post);
    }

    public Post accumulate(org.eclipse.jdt.core.dom.Statement stmt) {
        if (stmt instanceof org.eclipse.jdt.core.dom.ExpressionStatement) {
            org.eclipse.jdt.core.dom.Expression expr = ((org.eclipse.jdt.core.dom.ExpressionStatement) stmt).getExpression();
            if (expr instanceof org.eclipse.jdt.core.dom.MethodInvocation) {
                return fromMethodInvocation((org.eclipse.jdt.core.dom.MethodInvocation) expr);
            }
            if (expr instanceof org.eclipse.jdt.core.dom.Assignment) {
                return fromAssignment((org.eclipse.jdt.core.dom.Assignment) expr);
            }
        }
        if (stmt instanceof org.eclipse.jdt.core.dom.VariableDeclarationStatement) {
            return fromVariableDecl((org.eclipse.jdt.core.dom.VariableDeclarationStatement) stmt);
        }
        if (stmt instanceof org.eclipse.jdt.core.dom.IfStatement) {
            return fromIfStatement((org.eclipse.jdt.core.dom.IfStatement) stmt);
        }
        return Post.top();
    }

    private Post fromMethodInvocation(org.eclipse.jdt.core.dom.MethodInvocation expr) {
        String fn = expr.getName().getIdentifier();
        Post catalogPost = seedCatalog.get(fn);
        return catalogPost != null ? catalogPost : Post.top();
    }

    private Post fromAssignment(org.eclipse.jdt.core.dom.Assignment expr) {
        return Post.of(expr.toString());
    }

    private Post fromVariableDecl(org.eclipse.jdt.core.dom.VariableDeclarationStatement stmt) {
        org.eclipse.jdt.core.dom.VariableDeclarationFragment frag = (org.eclipse.jdt.core.dom.VariableDeclarationFragment) stmt.fragments().get(0);
        if (frag.getInitializer() instanceof org.eclipse.jdt.core.dom.MethodInvocation) {
            return fromMethodInvocation((org.eclipse.jdt.core.dom.MethodInvocation) frag.getInitializer());
        }
        return Post.top();
    }

    private Post fromIfStatement(org.eclipse.jdt.core.dom.IfStatement stmt) {
        Post thenPost = stmt.getThenStatement() != null
            ? accumulate(stmt.getThenStatement())
            : Post.top();
        Post elsePost = stmt.getElseStatement() != null
            ? accumulate(stmt.getElseStatement())
            : Post.top();
        return mergePosts(thenPost, elsePost);
    }

    private Post mergePosts(Post a, Post b) {
        if (a.isTop && b.isTop) return Post.top();
        if (a.isTop) return b;
        if (b.isTop) return a;
        java.util.List<String> merged = new java.util.ArrayList<>(a.constraints);
        merged.addAll(b.constraints);
        return new Post(merged, false);
    }

    public DiagnosticResult checkCallsite(String calleeId, Post currentPost) {
        if (currentPost.isTop) return null;
        Post calleePre = seedCatalog.get(calleeId);
        if (calleePre == null) return null;
        for (String c : currentPost.constraints) {
            if (!calleePre.constraints.contains(c)) {
                return new DiagnosticResult("implication-failed",
                    "post does not imply callee pre: " + String.join(" && ", calleePre.constraints));
            }
        }
        return null;
    }

    public static class DiagnosticResult {
        public final String code;
        public final String message;

        public DiagnosticResult(String code, String message) {
            this.code = code;
            this.message = message;
        }
    }
}