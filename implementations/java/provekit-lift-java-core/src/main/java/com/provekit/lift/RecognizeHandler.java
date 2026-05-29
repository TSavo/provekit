package com.provekit.lift;

import com.github.javaparser.JavaParser;
import com.github.javaparser.ParseResult;
import com.github.javaparser.ast.CompilationUnit;
import com.github.javaparser.ast.body.MethodDeclaration;
import com.github.javaparser.ast.expr.AnnotationExpr;
import com.github.javaparser.ast.expr.Expression;
import com.github.javaparser.ast.expr.MemberValuePair;
import com.github.javaparser.ast.expr.NormalAnnotationExpr;
import com.github.javaparser.ast.expr.StringLiteralExpr;
import com.provekit.ir.Jcs;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.stream.Stream;

public final class RecognizeHandler {
    private RecognizeHandler() {}

    public static Jcs.Obj recognizeImpl(Jcs.Obj params) {
        String projectRootText = params.stringFieldOrNull("project_root");
        if (projectRootText == null || projectRootText.isBlank()) {
            throw new IllegalArgumentException("missing `project_root`");
        }
        Path projectRoot = Path.of(projectRootText);
        List<String> sourcePaths = stringArray(params.get("source_paths"));
        List<SourceUnit> sources = sourceUnits(projectRoot, sourcePaths);
        Map<String, Jcs.Obj> bindingsByCid = new LinkedHashMap<>();
        if (params.get("binding_templates") instanceof Jcs.Arr bindingTemplates) {
            for (Jcs.Json value : bindingTemplates.values()) {
                if (!(value instanceof Jcs.Obj binding)) continue;
                String cid = binding.stringFieldOrNull("template_cid");
                if (cid != null && !cid.isBlank()) {
                    bindingsByCid.put(cid, binding);
                }
            }
        }
        Set<String> sugarTemplateFiles = new HashSet<>();
        if (bindingsByCid.isEmpty()) {
            for (SourceUnit sourceUnit : sources) {
                for (MethodDeclaration method : sourceUnit.unit().findAll(MethodDeclaration.class)) {
                    Jcs.Obj binding = bindingFromSugarMethod(method);
                    if (binding == null) continue;
                    String cid = binding.stringFieldOrNull("template_cid");
                    if (cid != null && !cid.isBlank()) {
                        bindingsByCid.put(cid, binding);
                        sugarTemplateFiles.add(sourceUnit.relPath());
                    }
                }
            }
        }

        List<Jcs.Json> tags = new ArrayList<>();
        for (SourceUnit sourceUnit : sources) {
            if (sugarTemplateFiles.contains(sourceUnit.relPath())) continue;
            for (MethodDeclaration method : sourceUnit.unit().findAll(MethodDeclaration.class)) {
                if (method.getBody().isEmpty()) continue;
                JavaAstTemplates.TemplateInfo candidate = JavaAstTemplates.fromMethod(method);
                Jcs.Obj binding = bindingsByCid.get(candidate.templateCid());
                if (binding != null) {
                    tags.add(tagFor(sourceUnit.relPath(), method, candidate, binding));
                }
            }
        }
        return Jcs.object("tags", Jcs.array(tags));
    }

    private record SourceUnit(String relPath, CompilationUnit unit) {}

    private static List<SourceUnit> sourceUnits(Path projectRoot, List<String> sourcePaths) {
        List<SourceUnit> out = new ArrayList<>();
        for (String relPath : sourcePaths) {
            for (SourcePath sourcePath : expandSourcePath(projectRoot, relPath)) {
                String source;
                try {
                    source = Files.readString(sourcePath.path());
                } catch (IOException e) {
                    System.err.println("recognize: skip unreadable source `" + sourcePath.relPath() + "`: " + e.getMessage());
                    continue;
                }
                ParseResult<CompilationUnit> result = new JavaParser().parse(source);
                if (!result.isSuccessful() || result.getResult().isEmpty()) {
                    System.err.println("recognize: skip unparseable source `" + sourcePath.relPath() + "`: " + result.getProblems());
                    continue;
                }
                out.add(new SourceUnit(sourcePath.relPath(), result.getResult().get()));
            }
        }
        return out;
    }

    private record SourcePath(String relPath, Path path) {}

    private static List<SourcePath> expandSourcePath(Path projectRoot, String relPath) {
        Path path = projectRoot.resolve(relPath).normalize();
        if (Files.isDirectory(path)) {
            try (Stream<Path> paths = Files.walk(path)) {
                return paths
                    .filter(Files::isRegularFile)
                    .filter(p -> p.getFileName().toString().endsWith(".java"))
                    .sorted()
                    .map(p -> new SourcePath(relativize(projectRoot, p), p))
                    .toList();
            } catch (IOException e) {
                System.err.println("recognize: skip unreadable source `" + relPath + "`: " + e.getMessage());
                return List.of();
            }
        }
        if (!Files.isRegularFile(path)) {
            return List.of();
        }
        return List.of(new SourcePath(relPath, path));
    }

    private static String relativize(Path projectRoot, Path path) {
        Path absoluteRoot = projectRoot.toAbsolutePath().normalize();
        Path absolutePath = path.toAbsolutePath().normalize();
        if (absolutePath.startsWith(absoluteRoot)) {
            return absoluteRoot.relativize(absolutePath).toString().replace('\\', '/');
        }
        return path.toString().replace('\\', '/');
    }

    private static Jcs.Obj bindingFromSugarMethod(MethodDeclaration method) {
        if (method.getBody().isEmpty()) return null;
        for (AnnotationExpr annotation : method.getAnnotations()) {
            if (!isSugarAnnotation(annotation)) continue;
            String concept = null;
            String library = null;
            String family = null;
            if (annotation instanceof NormalAnnotationExpr normal) {
                for (MemberValuePair pair : normal.getPairs()) {
                    String key = pair.getNameAsString();
                    String value = stringLiteralValue(pair.getValue());
                    if (value == null) continue;
                    switch (key) {
                        case "concept" -> concept = value;
                        case "library" -> library = value;
                        case "family" -> family = value;
                        default -> {}
                    }
                }
            }
            if (concept == null || concept.isBlank() || library == null || library.isBlank()) {
                continue;
            }
            JavaAstTemplates.TemplateInfo template = JavaAstTemplates.fromMethod(method);
            List<Object> fields = new ArrayList<>(List.of(
                "ast_template", template.astTemplate(),
                "concept_name", Jcs.string(concept),
                "library_tag", Jcs.string(library),
                "param_names", Jcs.array(template.paramNames().stream().map(Jcs::string).toList()),
                "target_library_tag", Jcs.string(library),
                "template_cid", Jcs.string(template.templateCid())
            ));
            if (family != null && !family.isBlank()) {
                fields.add("family");
                fields.add(Jcs.string(family));
            }
            return Jcs.object(fields.toArray());
        }
        return null;
    }

    private static boolean isSugarAnnotation(AnnotationExpr annotation) {
        String name = annotation.getNameAsString();
        return name.equals("ProveKitSugar") || name.endsWith(".ProveKitSugar");
    }

    private static String stringLiteralValue(Expression expression) {
        if (expression instanceof StringLiteralExpr literal) {
            return literal.getValue();
        }
        return null;
    }

    private static Jcs.Obj tagFor(
            String relPath,
            MethodDeclaration method,
            JavaAstTemplates.TemplateInfo candidate,
            Jcs.Obj binding) {
        List<Jcs.Json> paramBindings = new ArrayList<>();
        List<String> params = JavaAstTemplates.paramNames(method);
        for (int i = 0; i < params.size(); i++) {
            paramBindings.add(Jcs.object(
                "index", Jcs.integer(i + 1L),
                "source_text", Jcs.string(params.get(i))
            ));
        }

        int startLine = 0;
        int startCol = 0;
        int endLine = 0;
        int endCol = 0;
        if (method.getRange().isPresent()) {
            var range = method.getRange().get();
            startLine = range.begin.line;
            startCol = Math.max(0, range.begin.column - 1);
            endLine = range.end.line;
            endCol = Math.max(0, range.end.column - 1);
        }

        return Jcs.object(
            "file", Jcs.string(relPath),
            "span", Jcs.object(
                "start_line", Jcs.integer(startLine),
                "start_col", Jcs.integer(startCol),
                "end_line", Jcs.integer(endLine),
                "end_col", Jcs.integer(endCol)
            ),
            "function_name", Jcs.string(method.getNameAsString()),
            "concept_name", valueOrNull(binding, "concept_name"),
            "library_tag", libraryTag(binding),
            "family", valueOrNull(binding, "family"),
            "template_cid", Jcs.string(candidate.templateCid()),
            "contract_cid", valueOrNull(binding, "contract_cid"),
            "match_tier", Jcs.string("exact"),
            "param_bindings", Jcs.array(paramBindings)
        );
    }

    private static Jcs.Json libraryTag(Jcs.Obj binding) {
        Jcs.Json libraryTag = binding.get("library_tag");
        if (libraryTag != null) return libraryTag;
        return valueOrNull(binding, "target_library_tag");
    }

    private static Jcs.Json valueOrNull(Jcs.Obj obj, String key) {
        Jcs.Json value = obj.get(key);
        return value == null ? Jcs.nullValue() : value;
    }

    private static List<String> stringArray(Jcs.Json value) {
        List<String> out = new ArrayList<>();
        if (value instanceof Jcs.Arr array) {
            for (Jcs.Json item : array.values()) {
                if (item instanceof Jcs.Str string) {
                    out.add(string.value());
                }
            }
        }
        return out;
    }
}
