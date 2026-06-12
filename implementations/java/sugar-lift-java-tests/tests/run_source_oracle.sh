#!/usr/bin/env bash
# Focused source-oracle tests for the Java assertion kit.
set -euo pipefail

command -v javac >/dev/null 2>&1 || { echo "SKIP: no JDK on PATH"; exit 0; }
command -v java  >/dev/null 2>&1 || { echo "SKIP: no java on PATH"; exit 0; }

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
KIT="$(cd "$HERE/.." && pwd)"
OUT="$KIT/out"

echo "== build kit =="
bash "$KIT/build.sh" "$OUT" >/dev/null 2>&1

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP/work/src"
cat > "$TMP/work/src/Demo.java" <<'JAVA'
package src;

public final class Demo {
    static int twice(int x) {
        return x * 2;
    }
}
JAVA

cat > "$TMP/TestSourceOracle.java" <<'JAVA'
import com.sun.source.tree.*;
import com.sun.source.util.*;
import javax.tools.*;
import java.net.URI;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;

public final class TestSourceOracle {
    public static void main(String[] args) throws Exception {
        Path root = Path.of(args[0]).toAbsolutePath().normalize();
        Path file = root.resolve("src/Demo.java");
        String source = Files.readString(file, StandardCharsets.UTF_8);

        JavaCompiler compiler = ToolProvider.getSystemJavaCompiler();
        JavaFileObject fo = new SimpleJavaFileObject(file.toUri(), JavaFileObject.Kind.SOURCE) {
            @Override public CharSequence getCharContent(boolean ignoreEncodingErrors) {
                return source;
            }
        };
        JavacTask task = (JavacTask) compiler.getTask(
                null, null, d -> {}, List.of("-proc:none"), null, List.of(fo));
        Trees trees = Trees.instance(task);
        CompilationUnitTree unit = task.parse().iterator().next();
        SourcePositions positions = trees.getSourcePositions();
        MethodTree method = find(unit, "twice");

        JavaSourceOracle.SourceFragmentLocus locus =
                JavaSourceOracle.sourceFragmentLocusForMethod(
                        root, file, unit, method, positions);
        JavaSourceOracle.SourceMemento memento =
                JavaSourceOracle.sourceMementoOf(locus);

        String json = memento.toJson();
        require(json.contains("\"file\":\"src/Demo.java\""), json);
        require(json.contains("\"source_function_name\":\"twice\""), json);
        require(memento.sourceCid().startsWith("blake3-512:"), memento.sourceCid());
        require(memento.templateCid().startsWith("blake3-512:"), memento.templateCid());
        require(!json.contains("return x * 2"), "memento leaked source body: " + json);

        JavaSourceOracle.SourceFragment fragment =
                JavaSourceOracle.resolve(root, memento);
        require(fragment.bodyText().contains("return x * 2;"), fragment.bodyText());

        JavaSourceOracle.SourceMemento bad =
                memento.withSourceCid("blake3-512:" + "0".repeat(128));
        boolean refused = false;
        try {
            JavaSourceOracle.resolve(root, bad);
        } catch (JavaSourceOracle.SourceOracleRefusal expected) {
            refused = expected.getMessage().contains("source CID misaligned");
        }
        require(refused, "tampered source CID was not refused");

        System.out.println("PASS: Java SourceOracle emits lean mementos and resolves by recompute");
    }

    private static MethodTree find(CompilationUnitTree unit, String name) {
        final MethodTree[] found = {null};
        new TreeScanner<Void, Void>() {
            @Override public Void visitMethod(MethodTree node, Void unused) {
                if (node.getName().contentEquals(name)) found[0] = node;
                return super.visitMethod(node, unused);
            }
        }.scan(unit, null);
        if (found[0] == null) throw new AssertionError("missing method " + name);
        return found[0];
    }

    private static void require(boolean ok, String msg) {
        if (!ok) throw new AssertionError(msg);
    }
}
JAVA

javac \
  --add-exports jdk.compiler/com.sun.source.tree=ALL-UNNAMED \
  --add-exports jdk.compiler/com.sun.source.util=ALL-UNNAMED \
  -cp "$OUT" \
  -d "$TMP" \
  "$TMP/TestSourceOracle.java"

java \
  --add-exports jdk.compiler/com.sun.source.tree=ALL-UNNAMED \
  --add-exports jdk.compiler/com.sun.source.util=ALL-UNNAMED \
  -cp "$OUT:$TMP" \
  TestSourceOracle "$TMP/work"
