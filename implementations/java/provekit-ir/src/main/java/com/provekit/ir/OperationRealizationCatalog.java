package com.provekit.ir;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.HashMap;
import java.util.Map;
import java.util.stream.Stream;

/**
 * Loads operation-realization mementos from menagerie/concept-shapes/catalog/realizations/
 * and provides a concept_name → rhs_op_name lookup keyed by target language.
 *
 * The realizations map abstraction operators (concept:X) to per-language
 * kit operator names (rust:Y / java:Z). Both lift and lower consult the
 * map at runtime so the catalog is the single source of truth — no
 * hardcoded if-chains for which concept maps to which kit operator.
 *
 * See #1391 / mint_operation_realizations.py.
 */
public final class OperationRealizationCatalog {

    private OperationRealizationCatalog() {}

    private static volatile Map<String, String> JAVA_OP_MAP = null;
    private static volatile Map<String, String> RUST_OP_MAP = null;
    private static volatile Map<String, String> JAVA_REVERSE_MAP = null;

    /** concept_name → kit-op name for target_lang=java. Null if not realized. */
    public static String javaOpFor(String conceptName) {
        Map<String, String> cached = JAVA_OP_MAP;
        if (cached == null) {
            synchronized (OperationRealizationCatalog.class) {
                cached = JAVA_OP_MAP;
                if (cached == null) {
                    cached = buildMap("java");
                    JAVA_OP_MAP = cached;
                }
            }
        }
        return cached.get(conceptName);
    }

    /** Reverse: java kit-op name → concept_name. Used by the lift side. */
    public static String conceptForJavaOp(String kitOpName) {
        Map<String, String> cached = JAVA_REVERSE_MAP;
        if (cached == null) {
            synchronized (OperationRealizationCatalog.class) {
                cached = JAVA_REVERSE_MAP;
                if (cached == null) {
                    // Build by inverting JAVA_OP_MAP (which we ensure is loaded first).
                    Map<String, String> forward = new HashMap<>();
                    Map<String, String> existing = JAVA_OP_MAP;
                    if (existing == null) {
                        forward = buildMap("java");
                        JAVA_OP_MAP = forward;
                    } else {
                        forward = existing;
                    }
                    Map<String, String> reverse = new HashMap<>();
                    for (Map.Entry<String, String> e : forward.entrySet()) {
                        reverse.putIfAbsent(e.getValue(), e.getKey());
                    }
                    cached = reverse;
                    JAVA_REVERSE_MAP = cached;
                }
            }
        }
        return cached.get(kitOpName);
    }

    /** concept_name → kit-op name for target_lang=rust. */
    public static String rustOpFor(String conceptName) {
        Map<String, String> cached = RUST_OP_MAP;
        if (cached == null) {
            synchronized (OperationRealizationCatalog.class) {
                cached = RUST_OP_MAP;
                if (cached == null) {
                    cached = buildMap("rust");
                    RUST_OP_MAP = cached;
                }
            }
        }
        return cached.get(conceptName);
    }

    private static Map<String, String> buildMap(String targetLang) {
        Map<String, String> out = new HashMap<>();
        Path cwd = Paths.get(System.getProperty("user.dir", "."));
        Path root = null;
        for (Path p = cwd; p != null; p = p.getParent()) {
            if (Files.isDirectory(p.resolve("menagerie"))) { root = p; break; }
        }
        if (root == null) return out;
        Path realizationsDir = root.resolve("menagerie")
                .resolve("concept-shapes").resolve("catalog").resolve("realizations");
        if (!Files.isDirectory(realizationsDir)) return out;
        try (Stream<Path> files = Files.list(realizationsDir)) {
            for (Path file : (Iterable<Path>) files::iterator) {
                String name = file.getFileName().toString();
                if (!name.endsWith(".json")) continue;
                try {
                    String raw = Files.readString(file, StandardCharsets.UTF_8);
                    Jcs.Json doc = Jcs.parse(raw);
                    if (!(doc instanceof Jcs.Obj envelope)) continue;
                    Jcs.Json mementoJ = envelope.get("memento");
                    if (!(mementoJ instanceof Jcs.Obj memento)) continue;
                    String role = memento.stringFieldOrNull("role");
                    if (!"abstraction-realization".equals(role)) continue;
                    String tlang = memento.stringFieldOrNull("target_lang");
                    if (!targetLang.equals(tlang)) continue;
                    Jcs.Json postJ = memento.get("post");
                    if (!(postJ instanceof Jcs.Obj post)) continue;
                    Jcs.Json lhsJ = post.get("lhs");
                    Jcs.Json rhsJ = post.get("rhs");
                    if (!(lhsJ instanceof Jcs.Obj lhs) || !(rhsJ instanceof Jcs.Obj rhs)) continue;
                    String conceptName = lhs.stringFieldOrNull("name");
                    String kitOpName = rhs.stringFieldOrNull("name");
                    if (conceptName == null || kitOpName == null) continue;
                    out.putIfAbsent(conceptName, kitOpName);
                } catch (IOException | IllegalArgumentException ignored) {
                    // Skip malformed; other files still load.
                }
            }
        } catch (IOException ignored) {
            // Empty map; dispatch falls through to non-catalog handlers.
        }
        return out;
    }
}
