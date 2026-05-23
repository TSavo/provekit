import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ObjectNode;
import com.fasterxml.jackson.databind.node.ArrayNode;
import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.nio.file.Path;
import org.bouncycastle.crypto.digests.Blake3Digest;

public final class CrossPlatform {
    static final ObjectMapper MAPPER = new ObjectMapper();
    static final BufferedReader STDIN_READER = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
    static String PLUGIN_VERSION = "0.1.0";
    static String PROTOCOL_VERSION = "pep/1.7.0";
    static String IR_VERSION = "v1.1.0";
    static final byte[] HEX = "0123456789abcdef".getBytes(StandardCharsets.UTF_8);
    public interface AdapterLifter {
        String name();
        String surface();
        JsonNode lift(Path workspaceRoot, java.util.List<String> sourcePaths);
    }
    static String stdin_read_line() { try { return STDIN_READER.readLine(); } catch (IOException e) { return null; } }
    static void stdout_write_line(String s) { System.out.println(s); }
    static void stderr_write_line(String s) { System.err.println(s); }
    static JsonNode json_parse(String s) { try { return MAPPER.readTree(s); } catch (Exception e) { return null; } }
    static String json_serialize(JsonNode v) { try { return MAPPER.writeValueAsString(v); } catch (Exception e) { return null; } }
    static byte[] blake3_512_of(byte[] b) { Blake3Digest d = new Blake3Digest(512); d.update(b, 0, b.length); byte[] o = new byte[64]; d.doFinal(o, 0, 64); return o; }
    static String encode_jcs(JsonNode v) { try { return MAPPER.writeValueAsString(v); } catch (Exception e) { return "null"; } }
    // concept: concept:jsonrpc-ndjson-server-loop
    public static void run_server(AdapterLifter adapter) {
        stderr_write_line(String.format("%s listening on stdio (JSON-RPC 2.0, NDJSON)", adapter.name ()));
        while (true) {
            var line = stdin_read_line();
            if ((line) == (null)) {
                break;
            } else {
                ;
            }
            if (line.trim().isEmpty()) {
                continue;
            } else {
                ;
            }
            Object[] __provekit_tuple = handle_line(line, adapter);
            com.fasterxml.jackson.databind.JsonNode response = (com.fasterxml.jackson.databind.JsonNode) __provekit_tuple[0];
            boolean stop = (boolean) __provekit_tuple[1];
            var response_str = (json_serialize(response) != null ? json_serialize(response) : String.format("{{\"jsonrpc\":\"2.0\",\"id\":null,\"error\":{{\"code\":-32603,\"message\":\"%s\"}}}}", null));
            stdout_write_line(response_str);
            if (stop) {
                break;
            } else {
                ;
            }
        }
    }
    // concept: concept:jsonrpc-request-dispatch
    public static Object[] handle_line(String line, AdapterLifter adapter) {
        var __provekit_v0 = json_parse(line);
        com.fasterxml.jackson.databind.JsonNode req;
        if ((__provekit_v0 != null)) {
            req = __provekit_v0;
        } else if ((__provekit_v0 != null)) {
            return new Object[] {error_response(MAPPER.nullNode(), -(32700), String.format("parse error: %s", __provekit_v0)), false};
        } else { req = null; }
        var id = (req.get("id").deepCopy() != null ? req.get("id").deepCopy() : MAPPER.nullNode());
        var method = ((req.get("method") != null ? req.get("method").asText() : null) != null ? (req.get("method") != null ? req.get("method").asText() : null) : "");
        var params = (req.get("params").deepCopy() != null ? req.get("params").deepCopy() : MAPPER.nullNode());
        var __provekit_v1 = method;
        if ((__provekit_v1 != null && __provekit_v1.equals("initialize"))) {
            return new Object[] {ok_response(id, initialize_result(adapter)), false};
        } else if ((__provekit_v1 != null && __provekit_v1.equals("lift"))) {
            return (((java.util.function.Supplier<Object[]>) () -> { var __provekit_v2 = lift(params, adapter); return (__provekit_v2 != null) ? new Object[] {ok_response(id, __provekit_v2), false} : ((__provekit_v2 != null) ? new Object[] {error_response(id, -(32602), String.valueOf(__provekit_v2)), false} : ((__provekit_v2 != null) ? new Object[] {error_response(id, -(32603), String.valueOf(__provekit_v2)), false} : null)); }).get());
        } else if ((__provekit_v1 != null && __provekit_v1.equals("shutdown"))) {
            return new Object[] {ok_response(id, MAPPER.nullNode()), true};
        } else if ((__provekit_v1 != null && __provekit_v1.equals(""))) {
            return new Object[] {error_response(id, -(32600), "missing `method` field"), false};
        } else {
            return new Object[] {error_response(id, -(32601), String.format("unknown method: %s", __provekit_v1)), false};
        }
    }
    // concept: concept:jsonrpc-initialize-response
    public static com.fasterxml.jackson.databind.JsonNode initialize_result(AdapterLifter adapter) {
        return ((java.util.function.Supplier<com.fasterxml.jackson.databind.JsonNode>)() -> { com.fasterxml.jackson.databind.node.ObjectNode __obj1 = MAPPER.createObjectNode(); __obj1.set("name", MAPPER.valueToTree(adapter.name ())); __obj1.set("version", MAPPER.valueToTree(PLUGIN_VERSION)); __obj1.set("protocol_version", MAPPER.valueToTree(PROTOCOL_VERSION)); __obj1.set("capabilities", ((java.util.function.Supplier<com.fasterxml.jackson.databind.JsonNode>)() -> { com.fasterxml.jackson.databind.node.ObjectNode __obj2 = MAPPER.createObjectNode(); __obj2.set("authoring_surfaces", ((java.util.function.Supplier<com.fasterxml.jackson.databind.JsonNode>)() -> { com.fasterxml.jackson.databind.node.ArrayNode __arr3 = MAPPER.createArrayNode(); __arr3.add(adapter.surface ()); return __arr3; }).get()); __obj2.set("ir_version", MAPPER.valueToTree(IR_VERSION)); __obj2.put("emits_signed_mementos", false); return __obj2; }).get()); return __obj1; }).get();
    }
    // concept: concept:lift-method-handler
    public static com.fasterxml.jackson.databind.JsonNode lift(com.fasterxml.jackson.databind.JsonNode params, AdapterLifter adapter) {
        var workspace_root = java.util.Objects.requireNonNullElseGet((params.get("workspace_root") != null ? ((java.util.function.Function)((java.util.function.Function<com.fasterxml.jackson.databind.JsonNode, String>) com.fasterxml.jackson.databind.JsonNode::asText)).apply(params.get("workspace_root")) : null), () -> { throw new RuntimeException(String.valueOf(String.valueOf("missing `workspace_root`"))); });
        var source_paths_raw = java.util.Objects.requireNonNullElseGet((params.get("source_paths") != null ? ((java.util.function.Function<com.fasterxml.jackson.databind.JsonNode, com.fasterxml.jackson.databind.JsonNode>) (n -> n != null && n.isArray() ? n : null)).apply(params.get("source_paths")) : null), () -> { throw new RuntimeException(String.valueOf(String.valueOf("missing `source_paths`"))); });
        java.util.List source_paths = java.util.stream.StreamSupport.stream(source_paths_raw.spliterator(), false).map(((java.util.function.Function<com.fasterxml.jackson.databind.JsonNode, Object>)((v) -> (v.asText() != null ? String.valueOf(v.asText()) : null)))).filter(java.util.Objects::nonNull).collect(java.util.stream.Collectors.toList());
        var options = (params.get("options").deepCopy() != null ? params.get("options").deepCopy() : MAPPER.nullNode());
        var emit = ((options.get("emit") != null ? ((java.util.function.Function)((java.util.function.Function<com.fasterxml.jackson.databind.JsonNode, String>) com.fasterxml.jackson.databind.JsonNode::asText)).apply(options.get("emit")) : null) != null ? (options.get("emit") != null ? ((java.util.function.Function)((java.util.function.Function<com.fasterxml.jackson.databind.JsonNode, String>) com.fasterxml.jackson.databind.JsonNode::asText)).apply(options.get("emit")) : null) : "ir-document");
        if (!java.util.Objects.equals(emit, "ir-document")) {
            throw new RuntimeException(String.valueOf(String.valueOf(String.format("emit mode `%s` not implemented (only `ir-document` is supported in this version)", emit))));
        } else {
            ;
        }
        java.nio.file.Path root = java.nio.file.Path.of(workspace_root.toString());
        return build_ir_document(root, source_paths, adapter);
    }
    // concept: concept:ir-document-assembly
    public static com.fasterxml.jackson.databind.JsonNode build_ir_document(java.nio.file.Path workspace_root, java.util.List<String> source_paths, AdapterLifter adapter) {
        var __provekit_struct = adapter.lift(workspace_root, source_paths);
        var mementos = __provekit_struct.get("mementos");
        var diagnostics = __provekit_struct.get("diagnostics");
        java.util.ArrayList ir_entries = new java.util.ArrayList<>();
        java.util.TreeSet seen_names = new java.util.TreeSet<>();
        for (var memento : mementos) {
            var original_name = ((memento.get("name") != null ? ((java.util.function.Function)((java.util.function.Function<com.fasterxml.jackson.databind.JsonNode, String>) com.fasterxml.jackson.databind.JsonNode::asText)).apply(memento.get("name")) : null) != null ? (memento.get("name") != null ? ((java.util.function.Function)((java.util.function.Function<com.fasterxml.jackson.databind.JsonNode, String>) com.fasterxml.jackson.databind.JsonNode::asText)).apply(memento.get("name")) : null) : "").toString();
            String addressed_name = content_addressed_name(original_name, memento);
            if (!(seen_names.add(addressed_name))) {
                continue;
            } else {
                ;
            }
            var map = memento;
            if ((map) != (null)) {
                return ((com.fasterxml.jackson.databind.node.ObjectNode) map).set("name".toString(), MAPPER.valueToTree(addressed_name));
            } else {
                ;
            }
            ir_entries.add(memento);
        }
        return ((java.util.function.Supplier<com.fasterxml.jackson.databind.JsonNode>)() -> { com.fasterxml.jackson.databind.node.ObjectNode __obj1 = MAPPER.createObjectNode(); __obj1.put("kind", "ir-document"); __obj1.set("ir", MAPPER.valueToTree(ir_entries)); __obj1.set("diagnostics", MAPPER.valueToTree(diagnostics)); return __obj1; }).get();
    }
    // concept: concept:content-addressed-memento-name
    public static String content_addressed_name(String original_name, com.fasterxml.jackson.databind.JsonNode memento) {
        String inv_cid = slot_cid(memento, "inv");
        String pre_cid = slot_cid(memento, "pre");
        String post_cid = slot_cid(memento, "post");
        String composed = String.format("%s|%s|%s", inv_cid, pre_cid, post_cid);
        String content_cid = blake3_512_cid(composed.getBytes(java.nio.charset.StandardCharsets.UTF_8));
        return String.format("%s#%s", original_name, content_cid);
    }
    // concept: concept:formula-slot-content-cid
    public static String slot_cid(com.fasterxml.jackson.databind.JsonNode memento, String key) {
        var __provekit_v0 = memento.get(key);
        if ((__provekit_v0 != null)) {
            return blake3_512_cid(encode_jcs(__provekit_v0).getBytes(java.nio.charset.StandardCharsets.UTF_8));
        } else {
            return new String();
        }
    }
    // concept: concept:blake3-512-self-identifying-cid
    public static String blake3_512_cid(byte[] bytes) {
        var raw = blake3_512_of(bytes);
        StringBuilder s = new StringBuilder(("blake3-512:".length()) + (128));
        s.append("blake3-512:");
        for (var b : raw) {
            s.append((char) (HEX[(int) ((b) >> (4))]));
            s.append((char) (HEX[(int) ((b) & (15))]));
        }
        return s.toString();
    }
    // concept: concept:jsonrpc-success-response
    public static com.fasterxml.jackson.databind.JsonNode ok_response(com.fasterxml.jackson.databind.JsonNode id, com.fasterxml.jackson.databind.JsonNode result) {
        return ((java.util.function.Supplier<com.fasterxml.jackson.databind.JsonNode>)() -> { com.fasterxml.jackson.databind.node.ObjectNode __obj1 = MAPPER.createObjectNode(); __obj1.put("jsonrpc", "2.0"); __obj1.set("id", id); __obj1.set("result", result); return __obj1; }).get();
    }
    // concept: concept:jsonrpc-error-response
    public static com.fasterxml.jackson.databind.JsonNode error_response(com.fasterxml.jackson.databind.JsonNode id, long code, String message) {
        return ((java.util.function.Supplier<com.fasterxml.jackson.databind.JsonNode>)() -> { com.fasterxml.jackson.databind.node.ObjectNode __obj1 = MAPPER.createObjectNode(); __obj1.put("jsonrpc", "2.0"); __obj1.set("id", id); __obj1.set("error", ((java.util.function.Supplier<com.fasterxml.jackson.databind.JsonNode>)() -> { com.fasterxml.jackson.databind.node.ObjectNode __obj2 = MAPPER.createObjectNode(); __obj2.put("code", code); __obj2.put("message", message); return __obj2; }).get()); return __obj1; }).get();
    }
}
