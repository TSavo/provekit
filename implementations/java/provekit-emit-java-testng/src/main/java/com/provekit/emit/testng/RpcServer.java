package com.provekit.emit.testng;

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

/** PEP 1.7.0 newline-delimited JSON-RPC server for the TestNG emitter kit. */
public final class RpcServer {

    static final String CONTENT_JSON =
        "{"
        + "\"emits\":\"testng-assertions\","
        + "\"predicates\":["
        + "\"concept:eq\",\"concept:ne\",\"concept:lt\",\"concept:gt\","
        + "\"concept:le\",\"concept:ge\",\"concept:option-is-some\","
        + "\"concept:option-is-none\",\"concept:not-null\",\"concept:fallible-err\""
        + "],"
        + "\"target_framework\":\"testng\","
        + "\"target_language\":\"java\""
        + "}";

    static final String KIND = "emit";
    static final boolean CRITICAL = false;
    static final String VERSION = "0.1.0";
    static final List<String> PROTOCOL_VERSIONS = List.of("pep/1.7.0");
    static final String PROVENANCE_CID =
        "blake3-512:00000000000000000000000000000000000000000000000000000000"
        + "00000000000000000000000000000000000000000000000000000000000000000000000000";

    static final String PLUGIN_CID =
        "blake3-512:2ca927e1b43c8972148a36b11a68aff37f1d6493ef045d939b36a5064f68"
        + "ebc91172140f327c665efa727ec400f7ef3972b7702dceefea5b4321056e2773f484";

    private final BufferedReader in = new BufferedReader(new InputStreamReader(System.in));
    private final PrintWriter out = new PrintWriter(System.out, true);
    private final TestNgEmitter emitter = new TestNgEmitter();

    public void run() {
        try {
            String line;
            while ((line = in.readLine()) != null) {
                String trimmed = line.trim();
                if (!trimmed.isEmpty()) handle(trimmed);
            }
        } catch (IOException e) {
            System.err.println("emit-java-testng RPC read error: " + e.getMessage());
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
                    TestNgEmitter.Emission emission = emitter.emit(plan);
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

    String describeResult() {
        return "{"
            + "\"envelope\":{"
            + "\"declaredAt\":\"2026-05-29T00:00:00.000Z\","
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
            + "\"note\":\"Emits TestNG assertions that verify neutral ProofIR-shaped predicates. Predicate->assertion mapping is inline Java framework knowledge.\""
            + "}"
            + "}";
    }

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
                "testng check requires a pom.xml at or above " + outDir);
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
