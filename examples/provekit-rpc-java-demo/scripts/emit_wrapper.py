#!/usr/bin/env python3
"""Substrate-side java wrapper emitter.

Reads `provekit lift --library-bindings --json` output and emits a complete
java compilation unit:
  - package declaration + imports
  - class declaration with constants
  - AdapterLifter interface (from rust trait decl — TODO: lift extends to traits)
  - @boundary primitive stub methods (auto from bind-lift-entry sigs)

The substrate-honest version would have the java realize plugin emit this
directly when given the rust IR. This script proves the lift IR carries
enough data to do so. Run after `provekit lift --library-bindings` and
append the lowered @sugar functions from `provekit lower --target java`.

Usage:
  emit_wrapper.py <lift-ir.json> <output.java>
"""

import json
import sys


def map_rust_type_to_java(t: str) -> str:
    """Map rust source type to java type. The substrate-honest version
    walks the source-aliases catalog (per #1370); this is a demo-time
    shortcut covering the cross-platform crate's surface."""
    if not t:
        return "Object"
    t = t.strip()
    # Strip & and mut prefixes (rust borrow forms — java has reference-by-default).
    t = t.lstrip("&").lstrip("mut").strip()
    mapping = {
        "str": "String", "String": "String", "&str": "String",
        "i64": "long", "i32": "int",
        "Value": "com.fasterxml.jackson.databind.JsonNode",
        "&Value": "com.fasterxml.jackson.databind.JsonNode",
        "Vec<u8>": "byte[]", "&[u8]": "byte[]", "[u8]": "byte[]",
        "Option<String>": "String",  # rust None → java null
        "()": "void",
        "Path": "java.nio.file.Path", "&Path": "java.nio.file.Path",
        "&[String]": "java.util.List<String>",
        "LiftResult": "com.provekit.runtime.Result<com.fasterxml.jackson.databind.JsonNode, com.provekit.runtime.SumVariant>",
    }
    if t in mapping:
        return mapping[t]
    if t.startswith("Result<"):
        inner = t[7:-1]
        comma = inner.find(",")
        if comma > 0:
            ok = map_rust_type_to_java(inner[:comma].strip())
            err = "com.provekit.runtime.SumVariant"  # substrate erases err
            return f"com.provekit.runtime.Result<{ok}, {err}>"
    return t


def main():
    if len(sys.argv) != 3:
        print(f"usage: {sys.argv[0]} <lift-ir.json> <output.java>", file=sys.stderr)
        sys.exit(2)
    with open(sys.argv[1]) as f:
        ir = json.load(f)

    # Collect @boundary fns (bind-lift-entry that AREN'T also sugar entries).
    sugar_names = {
        e.get("source_function_name")
        for e in ir.get("ir", [])
        if e.get("kind") == "library-sugar-binding-entry"
    }
    boundary_fns = []
    for e in ir.get("ir", []):
        if e.get("kind") != "bind-lift-entry":
            continue
        fn = e.get("source_function_name")
        if not fn or fn in sugar_names:
            continue
        params = list(zip(e.get("param_names", []), e.get("param_types", [])))
        ret = map_rust_type_to_java(e.get("return_type", "()"))
        param_decls = ", ".join(f"{map_rust_type_to_java(t)} {n}" for n, t in params)
        boundary_fns.append((fn, param_decls, ret))

    out = []
    out.append("// AUTO-GENERATED from rust @boundary declarations via provekit lift")
    out.append("package com.provekit.crossplatform;")
    out.append("")
    out.append("import com.fasterxml.jackson.databind.JsonNode;")
    out.append("import com.fasterxml.jackson.databind.ObjectMapper;")
    out.append("import com.provekit.runtime.Result;")
    out.append("import com.provekit.runtime.SumVariant;")
    out.append("import com.provekit.runtime.Substrate;")
    out.append("import java.nio.file.Path;")
    out.append("")
    out.append("public final class CrossPlatform {")
    out.append("    public static final ObjectMapper MAPPER = new ObjectMapper();")
    out.append("    public static String PLUGIN_VERSION = \"0.1.0\";")
    out.append("    public static String PROTOCOL_VERSION = \"pep/1.7.0\";")
    out.append("    public static String IR_VERSION = \"v1.1.0\";")
    out.append("    public static final byte[] HEX = \"0123456789abcdef\".getBytes();")
    out.append("")
    out.append("    public interface AdapterLifter {")
    out.append("        String name();")
    out.append("        String surface();")
    out.append("        Result<JsonNode, SumVariant> lift(Path workspaceRoot, java.util.List<String> sourcePaths);")
    out.append("    }")
    out.append("")
    out.append("    // ─── @boundary primitives (auto-emitted from rust source) ───")
    for fn, params, ret in boundary_fns:
        out.append(f"    public static {ret} {fn}({params}) {{")
        out.append(f"        throw new UnsupportedOperationException(\"boundary stub: {fn}\");")
        out.append("    }")
    out.append("    // ─── @sugar functions appended below ───")
    out.append("}")

    with open(sys.argv[2], "w") as f:
        f.write("\n".join(out) + "\n")
    print(f"emit_wrapper: wrote {sys.argv[2]} ({len(boundary_fns)} @boundary stubs)")


if __name__ == "__main__":
    main()
