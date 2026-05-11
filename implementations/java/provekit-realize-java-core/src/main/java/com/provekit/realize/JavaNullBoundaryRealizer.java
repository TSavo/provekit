package com.provekit.realize;

import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.Optional;

import com.github.javaparser.JavaParser;
import com.github.javaparser.ParseResult;
import com.github.javaparser.ast.CompilationUnit;
import com.github.javaparser.ast.body.MethodDeclaration;
import com.github.javaparser.ast.body.Parameter;
import com.github.javaparser.ast.expr.StringLiteralExpr;
import com.provekit.ir.Blake3;
import com.provekit.lift.ContractDecl;
import com.provekit.lift.Extractor;
import com.provekit.lift.provekitnative.ProvekitNativeExtractor;
import com.provekit.lift.springweb.SpringWebExtractor;

public final class JavaNullBoundaryRealizer {
    public RealizerOutput realize(RealizerPlan plan) {
        if (!"transform".equals(plan.mode())) {
            return RealizerOutput.refusal(plan, "UNSUPPORTED_MODE");
        }
        if (blank(plan.targetSymbol()) || blank(plan.proofVar()) || blank(plan.source())) {
            return RealizerOutput.refusal(plan, "MISSING_HOST_CONTEXT");
        }

        Optional<String> transformed = switch (plan.surface()) {
            case "java-provekit-native" -> addProvekitRequires(plan.source(), plan.targetSymbol(), plan.proofVar());
            case "java-spring-web" -> addSpringRequestParam(plan.source(), plan.targetSymbol(), plan.proofVar());
            default -> Optional.empty();
        };
        if (transformed.isEmpty()) {
            return RealizerOutput.refusal(plan, "UNSUPPORTED_SURFACE_OR_TARGET");
        }

        String modifiedSource = transformed.get();
        LiftResult lift = liftAndCheck(plan.surface(), modifiedSource, plan.targetSymbol(), plan.proofVar());
        if (!lift.closed()) {
            return RealizerOutput.candidate(plan, "POST_LIFT_DID_NOT_CLOSE_GAP", modifiedSource);
        }

        String patchCid = cid(plan.source() + "\n---provekit-drop---\n" + modifiedSource);
        String artifactCid = cid(modifiedSource);
        String postLiftCid = cid(lift.postLiftJson());
        String closureWitnessJson = "{"
            + "\"kind\":\"TruthDischargeBodyClaim\","
            + "\"claimKind\":\"closure\","
            + "\"gapCid\":" + JsonUtil.quoted(plan.gapCid()) + ","
            + "\"policyCid\":" + JsonUtil.quoted(plan.policyCid()) + ","
            + "\"postLiftCid\":" + JsonUtil.quoted(postLiftCid) + ","
            + "\"sourcePredicate\":" + JsonUtil.quoted(plan.sourcePredicate()) + ","
            + "\"targetPredicate\":" + JsonUtil.quoted(plan.targetPredicate()) + ","
            + "\"transformedArtifactCid\":" + JsonUtil.quoted(artifactCid)
            + "}";
        String closureWitnessCid = cid(closureWitnessJson);

        return RealizerOutput.closed(
            plan,
            patchCid,
            artifactCid,
            postLiftCid,
            closureWitnessCid,
            closureWitnessJson,
            modifiedSource,
            lift.postLiftJson()
        );
    }

    private Optional<String> addProvekitRequires(String source, String methodName, String varName) {
        Optional<CompilationUnit> maybeCu = parse(source);
        if (maybeCu.isEmpty()) return Optional.empty();
        CompilationUnit cu = maybeCu.get();
        Optional<MethodDeclaration> method = findMethod(cu, methodName);
        if (method.isEmpty()) return Optional.empty();
        cu.addImport("com.provekit.contract.Requires");
        MethodDeclaration declaration = method.get();
        boolean parameterExists = declaration.getParameters().stream()
            .anyMatch(p -> p.getNameAsString().equals(varName));
        if (!parameterExists) return Optional.empty();
        boolean alreadyAnnotated = declaration.getAnnotations().stream()
            .anyMatch(ann -> ann.getNameAsString().endsWith("Requires"));
        if (!alreadyAnnotated) {
            declaration.addSingleMemberAnnotation("Requires", new StringLiteralExpr(varName + " != null"));
        }
        return Optional.of(cu.toString());
    }

    private Optional<String> addSpringRequestParam(String source, String methodName, String varName) {
        Optional<CompilationUnit> maybeCu = parse(source);
        if (maybeCu.isEmpty()) return Optional.empty();
        CompilationUnit cu = maybeCu.get();
        Optional<MethodDeclaration> method = findMethod(cu, methodName);
        if (method.isEmpty()) return Optional.empty();
        Optional<Parameter> parameter = method.get().getParameters().stream()
            .filter(p -> p.getNameAsString().equals(varName))
            .findFirst();
        if (parameter.isEmpty()) return Optional.empty();
        cu.addImport("org.springframework.web.bind.annotation.RequestParam");
        boolean alreadyAnnotated = parameter.get().getAnnotations().stream()
            .anyMatch(ann -> ann.getNameAsString().endsWith("RequestParam"));
        if (!alreadyAnnotated) {
            parameter.get().addMarkerAnnotation("RequestParam");
        }
        return Optional.of(cu.toString());
    }

    private LiftResult liftAndCheck(String surface, String source, String methodName, String varName) {
        Optional<CompilationUnit> maybeCu = parse(source);
        if (maybeCu.isEmpty()) return new LiftResult(false, "");
        CompilationUnit cu = maybeCu.get();
        Extractor extractor = switch (surface) {
            case "java-provekit-native" -> new ProvekitNativeExtractor();
            case "java-spring-web" -> new SpringWebExtractor();
            default -> null;
        };
        if (extractor == null) return new LiftResult(false, "");

        List<ContractDecl> declarations = extractor.extract(cu, source);
        String postLift = toIrDocumentJson(declarations);
        boolean closed = declarations.stream().anyMatch(decl ->
            decl.symbol.equals(methodName)
                && decl.preconditions.stream().anyMatch(pre -> isNonNullPrecondition(pre, varName))
        );
        return new LiftResult(closed, postLift);
    }

    private boolean isNonNullPrecondition(String preconditionJson, String varName) {
        return preconditionJson.contains("\"name\":\"neq\"")
            && preconditionJson.contains("\"name\":\"" + JsonUtil.escape(varName) + "\"")
            && preconditionJson.contains("\"value\":null");
    }

    private String toIrDocumentJson(List<ContractDecl> declarations) {
        List<String> parts = new ArrayList<>();
        for (ContractDecl declaration : declarations) {
            parts.add(declaration.toJson());
        }
        return "{\"kind\":\"ir-document\",\"ir\":[" + String.join(",", parts) + "],\"callEdges\":[],\"diagnostics\":[]}";
    }

    private Optional<CompilationUnit> parse(String source) {
        ParseResult<CompilationUnit> parsed = new JavaParser().parse(source);
        if (!parsed.isSuccessful() || parsed.getResult().isEmpty()) return Optional.empty();
        return parsed.getResult();
    }

    private Optional<MethodDeclaration> findMethod(CompilationUnit cu, String methodName) {
        return cu.findAll(MethodDeclaration.class).stream()
            .filter(method -> method.getNameAsString().equals(methodName))
            .findFirst();
    }

    private String cid(String text) {
        return Blake3.blake3_512(text.getBytes(StandardCharsets.UTF_8));
    }

    private boolean blank(String s) {
        return s == null || s.isBlank();
    }

    private record LiftResult(boolean closed, String postLiftJson) {}
}
