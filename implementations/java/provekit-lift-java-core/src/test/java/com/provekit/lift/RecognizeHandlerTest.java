package com.provekit.lift;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.provekit.ir.Jcs;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import org.junit.jupiter.api.Test;

class RecognizeHandlerTest {
    private static final String HTTP_CONTRACT_CID = "blake3-512:" + "a".repeat(128);
    private static final String SQL_CONTRACT_CID = "blake3-512:" + "b".repeat(128);

    @Test
    void recognizeEmitsExactTagForAlphaEquivalentUserMethod() throws Exception {
        JavaAstTemplates.TemplateInfo template = JavaAstTemplates.fromMethodSource("""
            Object shim(String url, Headers headers) {
              return client.execute(url, headers);
            }
            """);
        Jcs.Obj binding = binding(
            "concept:http-request",
            "provekit-shim-java-okhttp",
            "concept:family:http",
            template,
            HTTP_CONTRACT_CID
        );

        Path root = Files.createTempDirectory("recognize-java-http");
        String rel = "src/main/java/com/example/Handlers.java";
        write(root, rel, """
            package com.example;
            class Handlers {
              Object fetchUrl(String u, Headers h) {
                return client.execute(u, h);
              }
            }
            """);

        Jcs.Obj response = RecognizeHandler.recognizeImpl(params(root, List.of(rel), List.of(binding)));

        Jcs.Arr tags = response.arrayField("tags");
        assertEquals(1, tags.values().size(), Jcs.encode(response));
        Jcs.Obj tag = tags.objectAt(0);
        assertEquals(rel, tag.stringField("file"));
        assertEquals("fetchUrl", tag.stringField("function_name"));
        assertEquals("concept:http-request", tag.stringField("concept_name"));
        assertEquals("provekit-shim-java-okhttp", tag.stringField("library_tag"));
        assertEquals("concept:family:http", tag.stringField("family"));
        assertEquals(template.templateCid(), tag.stringField("template_cid"));
        assertEquals(HTTP_CONTRACT_CID, tag.stringField("contract_cid"));
        assertEquals("exact", tag.stringField("match_tier"));
        assertTrue(numberField(tag.objectField("span"), "start_line") > 0, Jcs.encode(tag));

        Jcs.Arr paramBindings = tag.arrayField("param_bindings");
        assertEquals(2, paramBindings.values().size());
        assertEquals(1, numberField(paramBindings.objectAt(0), "index"));
        assertEquals("u", paramBindings.objectAt(0).stringField("source_text"));
        assertEquals(2, numberField(paramBindings.objectAt(1), "index"));
        assertEquals("h", paramBindings.objectAt(1).stringField("source_text"));
    }

    @Test
    void recognizeReturnsEmptyTagsForNonMatchingSource() throws Exception {
        JavaAstTemplates.TemplateInfo template = JavaAstTemplates.fromMethodSource("""
            Object shim(String url, Headers headers) {
              return client.execute(url, headers);
            }
            """);
        Jcs.Obj binding = binding(
            "concept:http-request",
            "provekit-shim-java-okhttp",
            null,
            template,
            HTTP_CONTRACT_CID
        );

        Path root = Files.createTempDirectory("recognize-java-negative");
        String rel = "src/main/java/com/example/Handlers.java";
        write(root, rel, """
            package com.example;
            class Handlers {
              Object fetchUrl(String u, Headers h) {
                return client.send(u, h);
              }
            }
            """);

        Jcs.Obj response = RecognizeHandler.recognizeImpl(params(root, List.of(rel), List.of(binding)));

        assertTrue(response.arrayField("tags").isEmpty(), Jcs.encode(response));
    }

    @Test
    void recognizeRoutesMultipleBindingsPerCallSitePool() throws Exception {
        JavaAstTemplates.TemplateInfo httpTemplate = JavaAstTemplates.fromMethodSource("""
            Object shim(String url, Headers headers) {
              return client.execute(url, headers);
            }
            """);
        JavaAstTemplates.TemplateInfo sqlTemplate = JavaAstTemplates.fromMethodSource("""
            Object shim(Connection conn, String sql, Args args) {
              return conn.query(sql, args);
            }
            """);
        Jcs.Obj httpBinding = binding(
            "concept:http-request",
            "provekit-shim-java-okhttp",
            "concept:family:http",
            httpTemplate,
            HTTP_CONTRACT_CID
        );
        Jcs.Obj sqlBinding = binding(
            "concept:sql-query",
            "provekit-shim-java-jdbc",
            "concept:family:sql",
            sqlTemplate,
            SQL_CONTRACT_CID
        );

        Path root = Files.createTempDirectory("recognize-java-multi");
        String rel = "src/main/java/com/example/Handlers.java";
        write(root, rel, """
            package com.example;
            class Handlers {
              Object fetchUrl(String u, Headers h) {
                return client.execute(u, h);
              }

              Object query(Connection c, String q, Args a) {
                return c.query(q, a);
              }
            }
            """);

        Jcs.Obj response = RecognizeHandler.recognizeImpl(params(root, List.of(rel), List.of(httpBinding, sqlBinding)));

        Jcs.Arr tags = response.arrayField("tags");
        assertEquals(2, tags.values().size(), Jcs.encode(response));
        List<String> concepts = tags.values().stream()
            .map(Jcs.Obj.class::cast)
            .map(t -> t.stringField("concept_name"))
            .toList();
        assertTrue(concepts.contains("concept:http-request"), concepts.toString());
        assertTrue(concepts.contains("concept:sql-query"), concepts.toString());
    }

    @Test
    void recognizeSelfResolvesJavaSugarTemplatesWithoutBindingTemplates() throws Exception {
        Path root = Files.createTempDirectory("recognize-java-self-templates");
        String shimRel = "src/main/java/com/example/Shim.java";
        write(root, shimRel, """
            package com.example;
            class Shim {
              @ProveKitSugar(concept = "concept:http-request", library = "provekit-shim-java-okhttp")
              Object fetch(String url, Headers headers) {
                return client.execute(url, headers);
              }
            }
            """);
        String userRel = "src/main/java/com/example/Handlers.java";
        write(root, userRel, """
            package com.example;
            class Handlers {
              Object fetchUrl(String u, Headers h) {
                return client.execute(u, h);
              }
            }
            """);

        Jcs.Obj response = RecognizeHandler.recognizeImpl(paramsWithoutBindings(root, List.of(shimRel, userRel)));

        Jcs.Arr tags = response.arrayField("tags");
        assertEquals(1, tags.values().size(), Jcs.encode(response));
        Jcs.Obj tag = tags.objectAt(0);
        assertEquals(userRel, tag.stringField("file"));
        assertEquals("fetchUrl", tag.stringField("function_name"));
        assertEquals("concept:http-request", tag.stringField("concept_name"));
        assertEquals("provekit-shim-java-okhttp", tag.stringField("library_tag"));
        assertEquals("exact", tag.stringField("match_tier"));
        Jcs.Arr paramBindings = tag.arrayField("param_bindings");
        assertEquals("u", paramBindings.objectAt(0).stringField("source_text"));
        assertEquals("h", paramBindings.objectAt(1).stringField("source_text"));
    }

    @Test
    void recognizeSelfResolvedJavaSugarDoesNotTagNonMatchingSource() throws Exception {
        Path root = Files.createTempDirectory("recognize-java-self-negative");
        String shimRel = "src/main/java/com/example/Shim.java";
        write(root, shimRel, """
            package com.example;
            class Shim {
              @ProveKitSugar(concept = "concept:http-request", library = "provekit-shim-java-okhttp")
              Object fetch(String url, Headers headers) {
                return client.execute(url, headers);
              }
            }
            """);
        String userRel = "src/main/java/com/example/Handlers.java";
        write(root, userRel, """
            package com.example;
            class Handlers {
              Object fetchUrl(String u, Headers h) {
                return client.send(u, h);
              }
            }
            """);

        Jcs.Obj response = RecognizeHandler.recognizeImpl(paramsWithoutBindings(root, List.of(shimRel, userRel)));

        assertTrue(response.arrayField("tags").isEmpty(), Jcs.encode(response));
    }

    private static Jcs.Obj binding(
            String conceptName,
            String libraryTag,
            String family,
            JavaAstTemplates.TemplateInfo template,
            String contractCid) {
        List<Object> fields = new java.util.ArrayList<>();
        fields.add("concept_name");
        fields.add(Jcs.string(conceptName));
        fields.add("library_tag");
        fields.add(Jcs.string(libraryTag));
        fields.add("ast_template");
        fields.add(template.astTemplate());
        fields.add("template_cid");
        fields.add(Jcs.string(template.templateCid()));
        fields.add("param_names");
        fields.add(Jcs.array(template.paramNames().stream().map(Jcs::string).toList()));
        fields.add("contract_cid");
        fields.add(Jcs.string(contractCid));
        if (family != null) {
            fields.add("family");
            fields.add(Jcs.string(family));
        }
        return Jcs.object(fields.toArray());
    }

    private static Jcs.Obj params(Path root, List<String> sourcePaths, List<Jcs.Json> bindings) {
        return Jcs.object(
            "project_root", Jcs.string(root.toString()),
            "source_paths", Jcs.array(sourcePaths.stream().map(Jcs::string).toList()),
            "binding_templates", Jcs.array(bindings)
        );
    }

    private static Jcs.Obj paramsWithoutBindings(Path root, List<String> sourcePaths) {
        return Jcs.object(
            "project_root", Jcs.string(root.toString()),
            "source_paths", Jcs.array(sourcePaths.stream().map(Jcs::string).toList())
        );
    }

    private static void write(Path root, String rel, String source) throws Exception {
        Path path = root.resolve(rel);
        Files.createDirectories(path.getParent());
        Files.writeString(path, source);
    }

    private static long numberField(Jcs.Obj obj, String key) {
        Jcs.Json value = obj.get(key);
        if (value instanceof Jcs.Num n) return n.value();
        throw new IllegalArgumentException("field is not a number: " + key);
    }
}
