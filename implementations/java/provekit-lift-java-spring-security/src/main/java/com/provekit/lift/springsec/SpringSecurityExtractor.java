package com.provekit.lift.springsec;

import java.util.*;
import com.github.javaparser.ast.*;
import com.github.javaparser.ast.body.*;
import com.github.javaparser.ast.expr.*;
import com.provekit.lift.*;

/**
 * Extracts authorization pre/postconditions from Spring Security annotations.
 *
 * Coq-relevant: these are runtime predicates over the security context.
 * @PreAuthorize("hasRole('ADMIN')") becomes a precondition on the
 * runtime authentication principal's granted authorities.
 *
 * The expression string is preserved as a predicate; a Coq model of
 * Spring Security can interpret hasRole, hasAuthority, isAuthenticated, etc.
 */
public class SpringSecurityExtractor implements Extractor {
    public String name() { return "spring-security"; }

    public List<ContractDecl> extract(CompilationUnit cu, String rawSource) {
        List<ContractDecl> out = new ArrayList<>();
        for (TypeDeclaration<?> type : cu.getTypes()) {
            for (BodyDeclaration<?> member : type.getMembers()) {
                if (member instanceof MethodDeclaration m) extractMethod(m, out);
            }
        }
        return out;
    }

    private void extractMethod(MethodDeclaration method, List<ContractDecl> out) {
        String symbol = method.getNameAsString();
        List<String> pres = new ArrayList<>();
        List<String> posts = new ArrayList<>();

        for (AnnotationExpr ann : method.getAnnotations()) {
            String name = simpleName(ann.getNameAsString());
            switch (name) {
                case "PreAuthorize" -> extractString(ann).ifPresent(expr ->
                    pres.add(secAtom("pre_authorize", expr)));
                case "PostAuthorize" -> extractString(ann).ifPresent(expr ->
                    posts.add(secAtom("post_authorize", expr)));
                case "Secured" -> {
                    // @Secured({"ROLE_ADMIN", "ROLE_USER"}) → disjunction
                    List<String> roles = extractStringArray(ann);
                    if (!roles.isEmpty()) {
                        if (roles.size() == 1) {
                            pres.add(secAtom("has_role", roles.get(0)));
                        } else {
                            pres.add(secOr(roles));
                        }
                    }
                }
                case "RolesAllowed" -> {
                    List<String> roles = extractStringArray(ann);
                    if (!roles.isEmpty()) {
                        if (roles.size() == 1) {
                            pres.add(secAtom("has_role", roles.get(0)));
                        } else {
                            pres.add(secOr(roles));
                        }
                    }
                }
            }
        }

        if (!pres.isEmpty() || !posts.isEmpty()) {
            out.add(new ContractDecl(symbol, pres, posts));
        }
    }

    private Optional<String> extractString(AnnotationExpr ann) {
        if (ann instanceof SingleMemberAnnotationExpr sma) {
            Expression e = sma.getMemberValue();
            if (e instanceof StringLiteralExpr sle) return Optional.of(sle.getValue());
        }
        if (ann instanceof NormalAnnotationExpr na) {
            for (MemberValuePair p : na.getPairs()) {
                if (p.getNameAsString().equals("value")) {
                    Expression e = p.getValue();
                    if (e instanceof StringLiteralExpr sle) return Optional.of(sle.getValue());
                }
            }
        }
        return Optional.empty();
    }

    private List<String> extractStringArray(AnnotationExpr ann) {
        List<String> result = new ArrayList<>();
        Expression value = null;
        if (ann instanceof SingleMemberAnnotationExpr sma) value = sma.getMemberValue();
        else if (ann instanceof NormalAnnotationExpr na) {
            for (MemberValuePair p : na.getPairs()) {
                if (p.getNameAsString().equals("value")) { value = p.getValue(); break; }
            }
        }
        if (value instanceof StringLiteralExpr sle) result.add(sle.getValue());
        else if (value instanceof ArrayInitializerExpr aie) {
            for (Expression e : aie.getValues()) {
                if (e instanceof StringLiteralExpr sle) result.add(sle.getValue());
            }
        }
        return result;
    }

    private String secAtom(String name, String expr) {
        return "{\"kind\":\"atomic\",\"name\":\""+name+"\",\"args\":[{\"kind\":\"const\",\"value\":\""+esc(expr)+"\",\"sort\":{\"kind\":\"primitive\",\"name\":\"String\"}}]}";
    }

    private String secOr(List<String> roles) {
        StringBuilder sb = new StringBuilder("{\"kind\":\"or\",\"operands\":[");
        for (int i=0;i<roles.size();i++) {
            if (i>0) sb.append(",");
            sb.append(secAtom("has_role", roles.get(i)));
        }
        sb.append("]}"); return sb.toString();
    }

    private String simpleName(String fq) {
        int dot = fq.lastIndexOf('.'); return dot >= 0 ? fq.substring(dot + 1) : fq;
    }
    private String esc(String s) { return s.replace("\\","\\\\").replace("\"","\\\""); }
}
