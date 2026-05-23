package com.provekit.lift.termshape;

import com.github.javaparser.StaticJavaParser;
import com.github.javaparser.ast.CompilationUnit;
import com.github.javaparser.ast.body.MethodDeclaration;
import com.provekit.ir.Jcs;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;

/**
 * CLI entrypoint for body-structural lift of java sources.
 *
 * <p>Usage:
 *   {@code java -jar provekit-lift-java-term-shape.jar <file.java> [--out <ir.json>]}
 *
 * <p>Produces a ProofIR document with one term_shape per method in the
 * input, plus an {@code observed_loss_record} naming every AST node the
 * lifter didn't yet have a recognizer for. The loss_record IS the
 * empirical work list — each entry says exactly which java pattern the
 * substrate emits but this lifter doesn't yet read.
 */
public final class Main {
    public static void main(String[] args) throws IOException {
        if (args.length == 0) {
            System.err.println("usage: provekit-lift-java-term-shape <file.java> [--out <ir.json>]");
            System.exit(2);
        }
        Path inputPath = Path.of(args[0]);
        Path outputPath = null;
        for (int i = 1; i < args.length; i++) {
            if ("--out".equals(args[i]) && i + 1 < args.length) {
                outputPath = Path.of(args[i + 1]);
                i++;
            }
        }
        String source = Files.readString(inputPath);
        CompilationUnit cu = StaticJavaParser.parse(source);

        TermShapeLifter lifter = new TermShapeLifter();
        List<Jcs.Json> entries = new ArrayList<>();
        List<Jcs.Json> losses = new ArrayList<>();

        for (MethodDeclaration method : cu.findAll(MethodDeclaration.class)) {
            // Stop at the @boundary — only lift @sugar bodies.
            // Substrate-honest: boundaries mark "lift gives up; realize
            // fills in." Walking into a boundary primitive's body is
            // off-substrate.
            //
            // Recognition: our java lower prefixes every @sugar method
            // with `// concept: concept:X`. Methods without this header
            // are boundaries (hand-written shim primitives) — emit as
            // boundary references, not lifted bodies.
            String conceptHeader = method.getComment()
                    .map(c -> c.getContent().trim())
                    .filter(c -> c.startsWith("concept:"))
                    .orElse(null);
            if (conceptHeader == null) {
                continue;
            }
            // Parse @substrate-signature comment if present. The java lower
            // emits this marker comment in the @sugar header carrying the
            // source-language signature metadata. Lift reads it to recover
            // visibility / generic params / original param types / sort
            // CIDs / source return type so the rust lower (or any other
            // source-language lower) can round-trip without external
            // metadata injection.
            String sigCommentBody = extractSubstrateSignature(method);
            SignatureMetadata sigMeta = SignatureMetadata.parseOrEmpty(sigCommentBody);
            // Self-declaration short-circuit: the method header declares
            // its concept. The IDENTITY of this method's term_shape IS
            // a concept-ref leaf to the declared concept. Both lifter
            // paths (citation-driven and syntax-driven) observe the
            // same header and produce the same leaf — convergence is
            // structural, not coincidental.
            //
            // The body walk still runs to produce realize metadata
            // (param_names, param_types, return_type, structural detail
            // attached as `body_shape`), but the canonical term_shape
            // at this level is the declared concept.
            //
            // If the concept isn't in the live catalogue, the
            // body_shape becomes the structural definition the
            // catalogue accepts on first sight — the concept is
            // lifted into existence.
            TermShapeLifter.LiftedMethod lifted = lifter.liftMethod(method);
            Jcs.Json termShape = Jcs.object(
                "concept_name", Jcs.string(conceptHeader),
                "kind", Jcs.string("concept-ref")
            );
            // Build the IR entry. When @substrate-signature was present,
            // populate the source-language fields from it; otherwise leave
            // them empty (lift fell back to java-as-source-language).
            List<Jcs.Field> entryFields = new ArrayList<>();
            entryFields.add(new Jcs.Field("kind", Jcs.string("lift-term-shape-entry")));
            entryFields.add(new Jcs.Field("function", Jcs.string(method.getNameAsString())));
            entryFields.add(new Jcs.Field("concept_name",
                Jcs.string(conceptHeader.replaceFirst("^concept:\\s*", ""))));
            entryFields.add(new Jcs.Field("term_shape", termShape));
            entryFields.add(new Jcs.Field("body_shape", lifted.termShape()));
            // Compute term_shape_cid as blake3-512 of JCS(body_shape) so
            // downstream lowers have a stable identity without needing
            // external metadata injection.
            String bodyShapeJcs = Jcs.encode(lifted.termShape());
            String bodyShapeCid = "blake3-512:" + blake3_512Hex(bodyShapeJcs.getBytes(java.nio.charset.StandardCharsets.UTF_8));
            entryFields.add(new Jcs.Field("term_shape_cid", Jcs.string(bodyShapeCid)));
            entryFields.add(new Jcs.Field("param_names", new Jcs.Arr(lifted.paramNames())));
            entryFields.add(new Jcs.Field("param_types", new Jcs.Arr(lifted.paramTypes())));
            entryFields.add(new Jcs.Field("return_type", Jcs.string(lifted.returnType())));
            // Substrate-signature fields (empty when comment absent).
            entryFields.add(new Jcs.Field("visibility", Jcs.string(sigMeta.visibility)));
            entryFields.add(new Jcs.Field("generic_params", Jcs.string(sigMeta.genericParams)));
            List<Jcs.Json> origTypes = new ArrayList<>();
            for (String t : sigMeta.originalParamTypes) origTypes.add(Jcs.string(t));
            entryFields.add(new Jcs.Field("original_param_types", new Jcs.Arr(origTypes)));
            List<Jcs.Json> sortCids = new ArrayList<>();
            for (String c : sigMeta.paramSortCids) sortCids.add(Jcs.string(c));
            entryFields.add(new Jcs.Field("param_sort_cids", new Jcs.Arr(sortCids)));
            entryFields.add(new Jcs.Field("return_sort_cid", Jcs.string(sigMeta.returnSortCid)));
            entryFields.add(new Jcs.Field("source_return_type", Jcs.string(sigMeta.sourceReturnType)));
            entries.add(new Jcs.Obj(entryFields));
            losses.addAll(lifted.lossRecords());
        }

        Jcs.Obj document = (Jcs.Obj) Jcs.object(
            "kind", Jcs.string("ir-document"),
            "sourceLanguage", Jcs.string("java"),
            "sourcePath", Jcs.string(inputPath.toString()),
            "ir", new Jcs.Arr(entries),
            "observed_loss_record", aggregateLosses(losses)
        );

        String encoded = Jcs.encode(document);
        if (outputPath != null) {
            Files.writeString(outputPath, encoded);
            System.err.println("lift: wrote " + outputPath
                + " (" + entries.size() + " term(s), "
                + losses.size() + " loss entries)");
        } else {
            System.out.println(encoded);
        }
    }

    /** Compute blake3-512 hex digest of a byte array using BouncyCastle.
     *  Used to mint substrate-canonical CIDs for term_shape during lift. */
    private static String blake3_512Hex(byte[] bytes) {
        try {
            // Try BouncyCastle Blake3Digest if available.
            Class<?> digestClass = Class.forName("org.bouncycastle.crypto.digests.Blake3Digest");
            Object digest = digestClass.getConstructor(int.class).newInstance(512);
            digestClass.getMethod("update", byte[].class, int.class, int.class)
                .invoke(digest, bytes, 0, bytes.length);
            byte[] out = new byte[64];
            digestClass.getMethod("doFinal", byte[].class, int.class).invoke(digest, out, 0);
            StringBuilder hex = new StringBuilder(128);
            for (byte b : out) hex.append(String.format("%02x", b & 0xFF));
            return hex.toString();
        } catch (Throwable t) {
            // Fallback: empty CID — downstream lower may refuse, but
            // the lift output stays well-formed.
            return "0".repeat(128);
        }
    }

    /** Walk orphan comments around the method declaration looking for the
     *  `@substrate-signature {...}` marker the java lower emits. JavaParser's
     *  `getComment()` only returns the immediate-preceding comment; the
     *  substrate-signature comment is a SECOND comment after the concept
     *  header, so we scan the file's line comments near the method. */
    private static String extractSubstrateSignature(MethodDeclaration method) {
        // JavaParser exposes all comments via getAllContainedComments + parent
        // navigation. The substrate-signature comment is placed immediately
        // before the method declaration, after the concept header. The
        // safest scan: walk all comments in the enclosing CompilationUnit
        // and find one positioned just before this method's line whose body
        // starts with "@substrate-signature ".
        if (method.getRange().isEmpty()) return null;
        int methodLine = method.getRange().get().begin.line;
        com.github.javaparser.ast.CompilationUnit cu = method.findCompilationUnit().orElse(null);
        if (cu == null) return null;
        String marker = "@substrate-signature";
        String best = null;
        int bestLine = -1;
        for (com.github.javaparser.ast.comments.Comment c : cu.getAllContainedComments()) {
            if (c.getRange().isEmpty()) continue;
            int line = c.getRange().get().begin.line;
            if (line >= methodLine) continue;
            String body = c.getContent().trim();
            if (body.startsWith(marker)) {
                if (line > bestLine) {
                    best = body;
                    bestLine = line;
                }
            }
        }
        return best;
    }

    /** Parsed @substrate-signature metadata. */
    static final class SignatureMetadata {
        String visibility = "";
        String genericParams = "";
        List<String> originalParamTypes = new ArrayList<>();
        List<String> paramSortCids = new ArrayList<>();
        String returnSortCid = "";
        String sourceReturnType = "";

        static SignatureMetadata parseOrEmpty(String body) {
            SignatureMetadata m = new SignatureMetadata();
            if (body == null) return m;
            int brace = body.indexOf('{');
            if (brace < 0) return m;
            String json = body.substring(brace);
            try {
                com.github.javaparser.ast.expr.Expression parsed = null;
                // Use a minimal JSON-ish parser. The substrate-signature
                // JSON is generated by jsonStringEscape — only basic types
                // (strings + string arrays). Walk char-by-char.
                java.util.Map<String, Object> kv = parseFlatJsonObject(json);
                Object v;
                if ((v = kv.get("visibility")) instanceof String s) m.visibility = s;
                if ((v = kv.get("genericParams")) instanceof String s) m.genericParams = s;
                if ((v = kv.get("returnSortCid")) instanceof String s) m.returnSortCid = s;
                if ((v = kv.get("sourceReturnType")) instanceof String s) m.sourceReturnType = s;
                if ((v = kv.get("originalParamTypes")) instanceof List<?> l)
                    for (Object o : l) if (o instanceof String s) m.originalParamTypes.add(s);
                if ((v = kv.get("paramSortCids")) instanceof List<?> l)
                    for (Object o : l) if (o instanceof String s) m.paramSortCids.add(s);
            } catch (Exception e) {
                // Malformed — fall back to empty metadata. The substrate
                // round-trip will still work, just without source-language
                // signature recovery for this method.
            }
            return m;
        }
    }

    /** Tiny ad-hoc parser for `{"k": "v", "k2": ["a", "b"]}` JSON objects.
     *  Only handles strings and string arrays (the substrate-signature
     *  schema). Returns Map<String, Object> where values are String or
     *  List<String>. */
    private static java.util.Map<String, Object> parseFlatJsonObject(String json) {
        java.util.Map<String, Object> out = new java.util.LinkedHashMap<>();
        int i = json.indexOf('{');
        if (i < 0) return out;
        i++;
        while (i < json.length()) {
            while (i < json.length() && Character.isWhitespace(json.charAt(i))) i++;
            if (i >= json.length() || json.charAt(i) == '}') break;
            if (json.charAt(i) != '"') break;
            int keyEnd = nextUnescapedQuote(json, i + 1);
            if (keyEnd < 0) break;
            String key = unescapeJsonString(json.substring(i + 1, keyEnd));
            i = keyEnd + 1;
            while (i < json.length() && (Character.isWhitespace(json.charAt(i)) || json.charAt(i) == ':')) i++;
            if (i >= json.length()) break;
            if (json.charAt(i) == '"') {
                int valEnd = nextUnescapedQuote(json, i + 1);
                if (valEnd < 0) break;
                out.put(key, unescapeJsonString(json.substring(i + 1, valEnd)));
                i = valEnd + 1;
            } else if (json.charAt(i) == '[') {
                i++;
                List<String> arr = new ArrayList<>();
                while (i < json.length()) {
                    while (i < json.length() && (Character.isWhitespace(json.charAt(i)) || json.charAt(i) == ',')) i++;
                    if (i >= json.length() || json.charAt(i) == ']') break;
                    if (json.charAt(i) != '"') break;
                    int e = nextUnescapedQuote(json, i + 1);
                    if (e < 0) break;
                    arr.add(unescapeJsonString(json.substring(i + 1, e)));
                    i = e + 1;
                }
                if (i < json.length() && json.charAt(i) == ']') i++;
                out.put(key, arr);
            } else {
                break;
            }
            while (i < json.length() && (Character.isWhitespace(json.charAt(i)) || json.charAt(i) == ',')) i++;
        }
        return out;
    }

    private static int nextUnescapedQuote(String s, int from) {
        for (int i = from; i < s.length(); i++) {
            char c = s.charAt(i);
            if (c == '\\') { i++; continue; }
            if (c == '"') return i;
        }
        return -1;
    }

    private static String unescapeJsonString(String s) {
        StringBuilder sb = new StringBuilder(s.length());
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (c == '\\' && i + 1 < s.length()) {
                char n = s.charAt(i + 1);
                switch (n) {
                    case '"':  sb.append('"'); i++; break;
                    case '\\': sb.append('\\'); i++; break;
                    case 'n':  sb.append('\n'); i++; break;
                    case 'r':  sb.append('\r'); i++; break;
                    case 't':  sb.append('\t'); i++; break;
                    default:   sb.append(c);
                }
            } else {
                sb.append(c);
            }
        }
        return sb.toString();
    }

    /** Aggregate loss entries by dimension (node_class) for the sidecar
     *  view. Each dimension lists every occurrence with source location. */
    private static Jcs.Json aggregateLosses(List<Jcs.Json> losses) {
        if (losses.isEmpty()) return Jcs.object();
        java.util.Map<String, List<Jcs.Json>> byDim = new java.util.TreeMap<>();
        for (Jcs.Json entry : losses) {
            if (!(entry instanceof Jcs.Obj obj)) continue;
            String dim = null;
            for (Jcs.Field f : obj.fields()) {
                if ("dimension".equals(f.key()) && f.value() instanceof Jcs.Str s) {
                    dim = s.value();
                }
            }
            if (dim == null) continue;
            byDim.computeIfAbsent(dim, k -> new ArrayList<>()).add(entry);
        }
        List<Jcs.Field> fields = new ArrayList<>();
        for (var e : byDim.entrySet()) {
            fields.add(new Jcs.Field(e.getKey(), new Jcs.Arr(e.getValue())));
        }
        return new Jcs.Obj(fields);
    }
}
