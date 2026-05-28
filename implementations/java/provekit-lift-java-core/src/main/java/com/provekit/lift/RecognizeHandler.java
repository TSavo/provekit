package com.provekit.lift;

import com.github.javaparser.JavaParser;
import com.github.javaparser.ParseResult;
import com.github.javaparser.ast.CompilationUnit;
import com.github.javaparser.ast.body.MethodDeclaration;
import com.provekit.ir.Jcs;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

public final class RecognizeHandler {
    private RecognizeHandler() {}

    public static Jcs.Obj recognizeImpl(Jcs.Obj params) {
        String projectRootText = params.stringFieldOrNull("project_root");
        if (projectRootText == null || projectRootText.isBlank()) {
            throw new IllegalArgumentException("missing `project_root`");
        }
        Path projectRoot = Path.of(projectRootText);
        List<String> sourcePaths = stringArray(params.get("source_paths"));
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

        List<Jcs.Json> tags = new ArrayList<>();
        for (String relPath : sourcePaths) {
            Path sourcePath = projectRoot.resolve(relPath).normalize();
            String source;
            try {
                source = Files.readString(sourcePath);
            } catch (IOException e) {
                System.err.println("recognize: skip unreadable source `" + relPath + "`: " + e.getMessage());
                continue;
            }
            ParseResult<CompilationUnit> result = new JavaParser().parse(source);
            if (!result.isSuccessful() || result.getResult().isEmpty()) {
                System.err.println("recognize: skip unparseable source `" + relPath + "`: " + result.getProblems());
                continue;
            }
            for (MethodDeclaration method : result.getResult().get().findAll(MethodDeclaration.class)) {
                if (method.getBody().isEmpty()) continue;
                JavaAstTemplates.TemplateInfo candidate = JavaAstTemplates.fromMethod(method);
                Jcs.Obj binding = bindingsByCid.get(candidate.templateCid());
                if (binding != null) {
                    tags.add(tagFor(relPath, method, candidate, binding));
                }
            }
        }
        return Jcs.object("tags", Jcs.array(tags));
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
