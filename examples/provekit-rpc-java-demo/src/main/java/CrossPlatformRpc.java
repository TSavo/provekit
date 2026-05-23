// CrossPlatformRpc.java
//
// A working Java RPC library lowered (in part) from
// implementations/rust/libprovekit-rpc-cross-platform/src/lib.rs.
//
// Functions marked [LOWERED] are verbatim output of `provekit lower
// --target java`. Functions marked [HAND-WRITTEN] cover constructs not
// yet in the substrate's lower vocabulary (tuple returns, generics,
// while-let, ?-operator, for-with-pattern, bit ops, etc.). The boundary
// primitives at the bottom come from the substrate's java-io shim.
//
// This is honest output: the substrate translated what it could; the
// rest needs vocabulary extension (filed) or hand-writing in the
// meantime. Both halves implement the SAME concept-hub identities.

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ObjectNode;
import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.UncheckedIOException;
import java.nio.charset.StandardCharsets;
import org.bouncycastle.crypto.digests.Blake3Digest;

public final class CrossPlatformRpc {
    static final ObjectMapper MAPPER = new ObjectMapper();
    static final BufferedReader STDIN_READER =
        new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));

    // ===== [BOUNDARY PRIMITIVES — java-io shim] =====

    public static String stdin_read_line_required() {
        try {
            String line = STDIN_READER.readLine();
            if (line == null) return null;
            return line;
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
    }

    public static void stdout_write_line(String line) {
        System.out.println(line);
    }

    public static void stderr_write_line(String line) {
        System.err.println(line);
    }

    public static JsonNode json_parse(String s) {
        try {
            return MAPPER.readTree(s);
        } catch (IOException e) {
            return null;
        }
    }

    public static String json_serialize(JsonNode v) {
        try {
            return MAPPER.writeValueAsString(v);
        } catch (Exception e) {
            return null;
        }
    }

    public static byte[] blake3_512_of(byte[] bytes) {
        Blake3Digest d = new Blake3Digest(512);
        d.update(bytes, 0, bytes.length);
        byte[] out = new byte[64];
        d.doFinal(out, 0, 64);
        return out;
    }

    public static String encode_jcs(JsonNode v) {
        try { return MAPPER.writeValueAsString(v); }
        catch (Exception e) { return "null"; }
    }

    // ===== [LOWERED — provekit lower output verbatim] =====

    // concept:jsonrpc-success-response (LOWERED verbatim from provekit lower)
    public static JsonNode ok_response(JsonNode id, JsonNode result) {
        return ((java.util.function.Supplier<JsonNode>)() -> { ObjectNode __obj1 = MAPPER.createObjectNode(); __obj1.put("jsonrpc", "2.0"); __obj1.set("id", id); __obj1.set("result", result); return __obj1; }).get();
    }

    // concept:jsonrpc-error-response (LOWERED verbatim from provekit lower)
    public static JsonNode error_response(JsonNode id, long code, String message) {
        return ((java.util.function.Supplier<JsonNode>)() -> { ObjectNode __obj1 = MAPPER.createObjectNode(); __obj1.put("jsonrpc", "2.0"); __obj1.set("id", id); __obj1.set("error", ((java.util.function.Supplier<JsonNode>)() -> { ObjectNode __obj2 = MAPPER.createObjectNode(); __obj2.put("code", code); __obj2.put("message", message); return __obj2; }).get()); return __obj1; }).get();
    }

    // concept:formula-slot-content-cid
    public static String slot_cid(JsonNode memento, String key) {
        return (((java.util.function.Supplier<Object>) () -> {
            var __provekit_v0 = memento.get(key);
            return (__provekit_v0 != null && !__provekit_v0.isNull())
                ? blake3_512_cid(encode_jcs(__provekit_v0).getBytes(StandardCharsets.UTF_8))
                : new String();
        }).get()).toString();
    }

    // concept:content-addressed-memento-name
    public static String content_addressed_name(String original_name, JsonNode memento) {
        String inv_cid = slot_cid(memento, "inv");
        String pre_cid = slot_cid(memento, "pre");
        String post_cid = slot_cid(memento, "post");
        String composed = String.format("%s|%s|%s", inv_cid, pre_cid, post_cid);
        String content_cid = blake3_512_cid(composed.getBytes(StandardCharsets.UTF_8));
        return String.format("%s#%s", original_name, content_cid);
    }

    // concept:blake3-512-self-identifying-cid (LOWERED verbatim — was hand-written; the lifter+lowerer now translate for-loop with pattern + bit-shifts + casts + StringBuilder→String coercion)
    public static String blake3_512_cid(byte[] bytes) {
        var raw = blake3_512_of(bytes);
        StringBuilder s = new StringBuilder(("blake3-512:".length()) + (128));
        s.append("blake3-512:");
        for (var b : raw) {
            s.append((char) (HEX_BYTES[(int) ((b) >> (4))]));
            s.append((char) (HEX_BYTES[(int) ((b) & (15))]));
        }
        return s.toString();
    }
    private static final byte[] HEX_BYTES = "0123456789abcdef".getBytes(StandardCharsets.UTF_8);

    // ===== [HAND-WRITTEN — vocabulary gaps still open] =====

    // concept:jsonrpc-initialize-response (needs json! with nested array literal)
    public static JsonNode initialize_result(String adapterName, String surface) {
        ObjectNode caps = MAPPER.createObjectNode();
        caps.putArray("authoring_surfaces").add(surface);
        caps.put("ir_version", "v1.1.0");
        caps.put("emits_signed_mementos", false);
        return MAPPER.createObjectNode()
            .put("name", adapterName)
            .put("version", "0.1.0")
            .put("protocol_version", "pep/1.7.0")
            .set("capabilities", caps);
    }

    // concept:jsonrpc-request-dispatch (needs tuple return + nested match)
    public static DispatchResult handle_line(String line, String adapterName, String surface) {
        JsonNode req = json_parse(line);
        if (req == null) {
            return new DispatchResult(error_response(MAPPER.nullNode(), -32700, "parse error"), false);
        }
        JsonNode idNode = req.get("id");
        JsonNode id = idNode != null ? idNode : MAPPER.nullNode();
        JsonNode methodNode = req.get("method");
        String method = methodNode != null ? methodNode.asText("") : "";
        switch (method) {
            case "initialize":
                return new DispatchResult(ok_response(id, initialize_result(adapterName, surface)), false);
            case "shutdown":
                return new DispatchResult(ok_response(id, MAPPER.nullNode()), true);
            case "":
                return new DispatchResult(error_response(id, -32600, "missing `method` field"), false);
            default:
                return new DispatchResult(error_response(id, -32601, "unknown method: " + method), false);
        }
    }

    public record DispatchResult(JsonNode response, boolean stop) {}

    // concept:jsonrpc-ndjson-server-loop (needs while-let + nested error handling)
    public static void run_server(String adapterName, String surface) {
        stderr_write_line(adapterName + " listening on stdio (JSON-RPC 2.0, NDJSON)");
        while (true) {
            String line = stdin_read_line_required();
            if (line == null) break;
            if (line.trim().isEmpty()) continue;
            DispatchResult r = handle_line(line, adapterName, surface);
            String responseStr = json_serialize(r.response);
            if (responseStr == null) {
                responseStr = "{\"jsonrpc\":\"2.0\",\"id\":null,\"error\":{\"code\":-32603,\"message\":\"serialize error\"}}";
            }
            stdout_write_line(responseStr);
            if (r.stop) break;
        }
    }

    public static void main(String[] args) {
        run_server("provekit-rpc-java-lowered-demo", "java-bind");
    }
}
