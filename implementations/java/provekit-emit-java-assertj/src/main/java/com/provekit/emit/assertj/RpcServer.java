package com.provekit.emit.assertj;

import java.io.BufferedReader;
import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.PrintWriter;
import java.io.UncheckedIOException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.CompletionException;

import com.provekit.ir.Blake3;
import com.provekit.ir.Jcs;

/**
 * PEP 1.7.0 newline-delimited JSON-RPC server for the AssertJ emitter plugin.
 *
 * <p>Reads one JSON-RPC request per line on stdin, writes one response per
 * line to stdout. Supported methods:
 * <ul>
 *   <li>{@code provekit.plugin.describe}  - plugin self-description.</li>
 *   <li>{@code provekit.plugin.invoke}    - emit an AssertJ test class from an
 *       {@link EmitPlan} carried in {@code params}; returns an
 *       {@link AssertJEmitter.Emission}.</li>
 *   <li>{@code provekit.plugin.shutdown}  - exit.</li>
 * </ul>
 *
 * <p>Mirrors the RpcServer shape in {@code provekit-realize-java-core} but is
 * deliberately smaller: there is no body-emit, no assembly, no platform
 * semantics. The emitter is a predicate -> assertion table plus a
 * test-class shell.
 */
public final class RpcServer {

    /**
     * The plugin-memento {@code header.content} payload (§4.2.1) for this
     * kit's {@code provekit.plugin.describe} response. This is the kit's
     * capability self-description: which neutral predicates it can emit
     * AssertJ assertions for. It is INLINE plugin self-description, NOT a
     * substrate catalog memento; framework knowledge stays in the kit.
     *
     * <p>It is the single source of truth for both the describe response and
     * the {@link #PLUGIN_CID} computation. JCS-canonicalizing this exact
     * object (with the surrounding header fields, {@code cid} elided) and
     * BLAKE3-512 hashing it MUST reproduce {@link #PLUGIN_CID} — enforced by
     * {@code RpcServerDescribeTest}. Changing it requires re-minting the CID
     * (via {@code mint-plugin-cid}) and updating the constant.
     */
    static final String CONTENT_JSON =
        "{"
        + "\"emits\":\"assertj-assertions\","
        + "\"predicates\":["
        + "\"concept:eq\",\"concept:ne\",\"concept:lt\",\"concept:gt\","
        + "\"concept:le\",\"concept:ge\",\"concept:option-is-some\","
        + "\"concept:option-is-none\""
        + "],"
        + "\"target_framework\":\"assertj\","
        + "\"target_language\":\"java\""
        + "}";

    static final String KIND = "emit";
    static final boolean CRITICAL = false;
    static final String VERSION = "0.1.0";
    static final List<String> PROTOCOL_VERSIONS = List.of("pep/1.7.0");
    static final String PROVENANCE_CID =
        "blake3-512:00000000000000000000000000000000000000000000000000000000"
        + "00000000000000000000000000000000000000000000000000000000000000000000000000";

    /**
     * PEP 1.7.0 plugin CID for this kit's describe header. Pre-computed by
     * {@code mint-plugin-cid} over the §6.1 cid-input
     * (JCS of {@code {content, critical, kind, protocol_versions,
     * provenance_cid, schemaVersion, version}}; the {@code cid} field is
     * elided). The strict loader ({@code provekit-plugin-loader/src/loader.rs})
     * recomputes and compares this; a mismatch refuses the load. Kept in lockstep
     * with {@link #CONTENT_JSON} by {@code RpcServerDescribeTest}, which recomputes
     * the CID in java (provekit-ir Jcs + Blake3, which are byte-identical to the
     * rust canonicalizer) and asserts equality.
     */
    static final String PLUGIN_CID =
        "blake3-512:be588f510c4af01b068e2925246d84a766cc2e01e64039ccd1311669fb51"
        + "f15791b2a0da8d8ddc07791e77e76b333c062860af00d7dceaa9cf98dfbcf0294032";

    private final BufferedReader in = new BufferedReader(new InputStreamReader(System.in));
    private final PrintWriter out = new PrintWriter(System.out, true);
    private final AssertJEmitter emitter = new AssertJEmitter();

    public void run() {
        try {
            String line;
            while ((line = in.readLine()) != null) {
                String trimmed = line.trim();
                if (!trimmed.isEmpty()) handle(trimmed);
            }
        } catch (IOException e) {
            System.err.println("emit-java-assertj RPC read error: " + e.getMessage());
        }
    }

    private void handle(String line) {
        String id = "null";
        String method = "";
        String params = "{}";
        try {
            Jcs.Json doc = Jcs.parse(line);
            if (doc instanceof Jcs.Obj obj) {
                Jcs.Json idJson = obj.get("id");
                if (idJson instanceof Jcs.Num n) id = Long.toString(n.value());
                else if (idJson instanceof Jcs.Str s) id = "\"" + s.value() + "\"";
                method = obj.stringFieldOrNull("method");
                if (method == null) method = "";
                Jcs.Json p = obj.get("params");
                if (p instanceof Jcs.Obj) params = Jcs.encode(p);
            }
        } catch (RuntimeException e) {
            sendError(id, -32700, "parse error: " + e.getMessage());
            return;
        }

        try {
            switch (method) {
                case "provekit.plugin.describe" -> sendResponse(id, describeResult());
                case "provekit.plugin.invoke" -> {
                    EmitPlan plan = EmitPlan.fromParams(params);
                    AssertJEmitter.Emission emission = emitter.emit(plan);
                    sendResponse(id, emission.toJson());
                }
                case "provekit.plugin.check" -> sendResponse(id, checkResult(params));
                case "provekit.plugin.shutdown" -> {
                    sendResponse(id, "null");
                    System.exit(0);
                }
                default -> sendError(id, -32601, "unknown method: " + method);
            }
        } catch (Exception e) {
            sendError(id, -32000,
                e.getMessage() != null ? e.getMessage() : e.getClass().getName());
        }
    }

    /**
     * Plugin self-description per PEP 1.7.0 §4.2.1: the JSON-RPC {@code result}
     * IS the plugin-memento body, an enveloped {@code {envelope, header,
     * metadata}} object. The strict loader
     * ({@code provekit-plugin-loader/src/loader.rs}) shape-validates these three
     * keys, deserializes the header, checks {@code schemaVersion == "1"},
     * checks {@code protocol_versions} intersects the runtime set, and
     * recomputes {@code header.cid} via §6.1 — refusing the load on any
     * mismatch. This response satisfies all of those so the kit loads through
     * the standard plugin loader (required by the #1436 retirement gauntlet,
     * which invokes this emitter through that loader).
     *
     * <p>Mirrors the enveloped shape of {@code provekit-realize-java-core}'s
     * {@code describeResult()}. The envelope signature is the all-zero
     * placeholder (the v0 loader parses but does not yet verify signatures).
     */
    String describeResult() {
        return "{"
            + "\"envelope\":{"
            + "\"declaredAt\":\"2026-05-23T00:00:00.000Z\","
            + "\"signature\":\"ed25519:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\","
            + "\"signer\":\"ed25519:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\""
            + "},"
            + "\"header\":{"
            + "\"cid\":\"" + PLUGIN_CID + "\","
            + "\"content\":" + CONTENT_JSON + ","
            + "\"critical\":" + CRITICAL + ","
            + "\"kind\":\"" + KIND + "\","
            + "\"protocol_versions\":[\"pep/1.7.0\"],"
            + "\"provenance_cid\":\"" + PROVENANCE_CID + "\","
            + "\"schemaVersion\":\"1\","
            + "\"version\":\"" + VERSION + "\""
            + "},"
            + "\"metadata\":{"
            + "\"note\":\"Emits AssertJ assertions that verify a contract's neutral predicates. Predicate->assertion mapping is inline Java framework knowledge.\""
            + "}"
            + "}";
    }

    /**
     * Recompute this kit's plugin CID exactly as the rust loader does (§6.1):
     * BLAKE3-512 of the JCS encoding of the cid-input map
     * {@code {content, critical, kind, protocol_versions, provenance_cid,
     * schemaVersion, version}} (the {@code cid} field is elided;
     * {@code protocol_versions} sorted ascending). provekit-ir's {@link Jcs}
     * encoder is byte-identical to rust's {@code provekit_canonicalizer}
     * (lexicographic codepoint key sort, integer rendering, RFC 8259 string
     * escaping), so this reproduces {@link #PLUGIN_CID}.
     */
    static String computePluginCid() {
        Jcs.Json content = Jcs.parse(CONTENT_JSON);
        List<String> pv = new ArrayList<>(PROTOCOL_VERSIONS);
        Collections.sort(pv);
        List<Jcs.Json> pvJson = new ArrayList<>();
        for (String v : pv) pvJson.add(Jcs.string(v));

        LinkedHashMap<String, Jcs.Json> fields = new LinkedHashMap<>();
        fields.put("content", content);
        fields.put("critical", Jcs.bool(CRITICAL));
        fields.put("kind", Jcs.string(KIND));
        fields.put("protocol_versions", Jcs.array(pvJson));
        fields.put("provenance_cid", Jcs.string(PROVENANCE_CID));
        fields.put("schemaVersion", Jcs.string("1"));
        fields.put("version", Jcs.string(VERSION));

        List<Jcs.Field> fieldList = new ArrayList<>();
        for (var e : fields.entrySet()) fieldList.add(new Jcs.Field(e.getKey(), e.getValue()));
        Jcs.Obj input = new Jcs.Obj(fieldList);
        return Blake3.blake3_512(Jcs.encodeUtf8(input));
    }

    private String checkResult(String paramsJson) throws IOException, InterruptedException {
        Jcs.Json parsed = Jcs.parse(paramsJson);
        if (!(parsed instanceof Jcs.Obj obj)) {
            throw new IllegalArgumentException("params must be an object");
        }
        String outDir = obj.stringFieldOrNull("out_dir");
        if (outDir == null || outDir.isBlank()) {
            throw new IllegalArgumentException("missing out_dir");
        }
        File projectRoot = findAncestorFile(new File(outDir), "pom.xml");
        if (projectRoot == null) {
            throw new IllegalArgumentException(
                "assertj check requires a pom.xml at or above " + outDir);
        }

        Process process = new ProcessBuilder("mvn", "-q", "test")
            .directory(projectRoot)
            .start();
        CompletableFuture<byte[]> stdoutFuture = readAllBytesAsync(process.getInputStream());
        CompletableFuture<byte[]> stderrFuture = readAllBytesAsync(process.getErrorStream());
        int exitCode = process.waitFor();
        byte[] stdout = joinBytes(stdoutFuture);
        byte[] stderr = joinBytes(stderrFuture);

        Jcs.Obj result = Jcs.object(
            "ok", Jcs.bool(exitCode == 0),
            "command", Jcs.string("mvn -q test"),
            "cwd", Jcs.string(projectRoot.getPath()),
            "stdout", Jcs.string(new String(stdout, StandardCharsets.UTF_8)),
            "stderr", Jcs.string(new String(stderr, StandardCharsets.UTF_8)),
            "exitCode", Jcs.integer(exitCode)
        );
        return Jcs.encode(result);
    }

    private static CompletableFuture<byte[]> readAllBytesAsync(InputStream stream) {
        return CompletableFuture.supplyAsync(() -> {
            try {
                return stream.readAllBytes();
            } catch (IOException e) {
                throw new UncheckedIOException(e);
            }
        });
    }

    private static byte[] joinBytes(CompletableFuture<byte[]> future) throws IOException {
        try {
            return future.join();
        } catch (CompletionException e) {
            if (e.getCause() instanceof UncheckedIOException io) throw io.getCause();
            throw e;
        }
    }

    private static File findAncestorFile(File start, String filename) {
        File cursor = start;
        while (cursor != null) {
            if (new File(cursor, filename).exists()) return cursor;
            cursor = cursor.getParentFile();
        }
        return null;
    }

    private void sendResponse(String id, String result) {
        out.println("{\"jsonrpc\":\"2.0\",\"id\":" + id + ",\"result\":" + result + "}");
    }

    private void sendError(String id, int code, String message) {
        out.println("{\"jsonrpc\":\"2.0\",\"id\":" + id
            + ",\"error\":{\"code\":" + code + ",\"message\":\"" + escape(message) + "\"}}");
    }

    private static String escape(String s) {
        if (s == null) return "";
        return s.replace("\\", "\\\\").replace("\"", "\\\"")
            .replace("\n", "\\n").replace("\r", "\\r");
    }
}
