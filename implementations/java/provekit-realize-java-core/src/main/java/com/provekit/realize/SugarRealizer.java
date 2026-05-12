package com.provekit.realize;

import java.util.List;

/**
 * Emits a canonical Java stub method wrapped in a per-function class.
 *
 * Mirrors cmd_transport.rs `realize_function` for TargetStyle::Java with
 * is_stub=true (the bind path where no term graph is available yet).
 *
 * Output format (per cmd_transport.rs Java branch):
 * <pre>
 * final class {PascalFn}Transported {
 *     // concept: {conceptName}
 *     public static {returnType} {function}({typedParams}) {
 *         throw new UnsupportedOperationException("provekit-bind canonical: {conceptName}");
 *     }
 * }
 * </pre>
 *
 * Slice 2: contract annotations (requires/ensures) are NOT emitted here;
 * those require contract lifting from source, which is out of scope for
 * the stub path.
 */
final class SugarRealizer {

    /**
     * Emit a Java stub for a single function.
     *
     * @param function    Rust snake_case function name (e.g. "wrap_identity")
     * @param params      Parameter names in order (e.g. ["x"])
     * @param paramTypes  Source-language (Rust) type strings (e.g. ["i64"])
     * @param returnType  Source-language return type string (e.g. "i64")
     * @param conceptName Concept binding name (e.g. "UNNAMED-CONCEPT-a777b12569a16b07")
     * @return Java source string, byte-identical to realize_for_bind("java", ...)
     */
    static String emitStub(
            String function,
            List<String> params,
            List<String> paramTypes,
            String returnType,
            String conceptName) {

        String className = snakeToPascal(function) + "Transported";
        String mappedReturn = mapSourceType(returnType);

        StringBuilder typedParamList = new StringBuilder();
        for (int i = 0; i < params.size(); i++) {
            String name = params.get(i);
            String srcType = i < paramTypes.size() ? paramTypes.get(i) : "i64";
            String mapped = mapSourceType(srcType);
            if (i > 0) typedParamList.append(", ");
            typedParamList.append(mapped).append(" ").append(name);
        }

        // annotation_prefix for Java: top_indent = "    "
        String annotationPrefix = "    // concept: " + conceptName + "\n";

        // stub body: 8 spaces indent
        String stubBody = "        throw new UnsupportedOperationException(\"provekit-bind canonical: " + conceptName + "\");\n";

        // Format matches cmd_transport.rs line 1474-1476:
        // "final class {class_name} {{\n{annotation_prefix}    public static {mapped_return} {function}({typed_param_list}) {{\n{body_str}    }}\n}}\n"
        return "final class " + className + " {\n"
                + annotationPrefix
                + "    public static " + mappedReturn + " " + function + "(" + typedParamList + ") {\n"
                + stubBody
                + "    }\n"
                + "}\n";
    }

    /**
     * Map a Rust source type to the Java equivalent.
     *
     * Mirrors cmd_transport.rs map_source_type for TargetStyle::Java.
     */
    static String mapSourceType(String src) {
        return switch (src) {
            case "()" -> "void";
            case "i64", "u64" -> "long";
            case "i32", "u32" -> "int";
            case "i16", "u16" -> "short";
            case "i8", "u8" -> "byte";
            case "f64" -> "double";
            case "f32" -> "float";
            case "bool" -> "boolean";
            case "String", "&str", "&String" -> "String";
            default -> src;
        };
    }

    /**
     * Convert snake_case to PascalCase.
     *
     * Mirrors cmd_transport.rs snake_to_pascal_local.
     */
    static String snakeToPascal(String s) {
        StringBuilder sb = new StringBuilder();
        for (String part : s.split("_", -1)) {
            if (part.isEmpty()) continue;
            sb.append(Character.toUpperCase(part.charAt(0)));
            if (part.length() > 1) sb.append(part.substring(1));
        }
        return sb.toString();
    }
}
