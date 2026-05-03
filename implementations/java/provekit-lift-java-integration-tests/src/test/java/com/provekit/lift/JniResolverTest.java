package com.provekit.lift;

import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

import com.github.javaparser.*;
import com.github.javaparser.ast.*;
import com.provekit.ir.CallEdgeDecl;

import java.util.*;

/**
 * Tests for JniResolver — Java JNI FFI call-site resolver per spec #114 R3.
 *
 * Tests mirror Go's cgo_resolver_test.go (PR #127) and Python's
 * test_ctypes_resolver.py (PR #131): same five structural tests, same
 * kit-resolution semantics, same byte-determinism requirement.
 */
public class JniResolverTest {

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    private CompilationUnit parse(String source) {
        ParseResult<CompilationUnit> result = new JavaParser().parse(source);
        assertTrue(result.isSuccessful() && result.getResult().isPresent(),
            "Failed to parse: " + result.getProblems());
        return result.getResult().get();
    }

    /** Build a minimal contract index with a known CID for the given symbol. */
    private Map<String, String> contractIndex(String... symbols) {
        Map<String, String> idx = new LinkedHashMap<>();
        for (String s : symbols) {
            idx.put(s, "blake3-512:deadbeef00000000-" + s.hashCode());
        }
        return idx;
    }

    private List<CallEdgeDecl> resolve(String source, Map<String, String> index) {
        CompilationUnit cu = parse(source);
        return JniResolver.resolve(cu, "Test.java", index);
    }

    // -----------------------------------------------------------------------
    // Test 1: System.loadLibrary("rust_callee") + native int process(int) +
    //         caller rb.process(n) -> call-edge targetSymbol = "rust-kit:RustBindings.process"
    // -----------------------------------------------------------------------

    @Test
    public void test1_rustCallee_nativeProcess_emitsRustKitEdge() {
        String source = """
            public class RustBindings {
                static {
                    System.loadLibrary("rust_callee");
                }
                public native int process(int n);
                public int caller(int value) {
                    return process(value);
                }
            }
            """;

        Map<String, String> index = contractIndex("RustBindings.caller");
        List<CallEdgeDecl> edges = resolve(source, index);

        assertEquals(1, edges.size(), "Expected exactly one call-edge");
        CallEdgeDecl edge = edges.get(0);

        assertEquals("rust-kit:RustBindings.process", edge.targetSymbol,
            "targetSymbol must be rust-kit:<ClassName>.<method>");
        assertNull(edge.targetContractCid,
            "Cross-kit call: targetContractCid must be null");
        assertEquals("blake3-512:deadbeef00000000-" + "RustBindings.caller".hashCode(),
            edge.sourceContractCid,
            "sourceContractCid must match the caller's contract CID");
        assertEquals("Test.java", edge.locusFile);
        assertTrue(edge.locusLine > 0, "locusLine must be positive");
    }

    // -----------------------------------------------------------------------
    // Test 2: System.loadLibrary("c_string_utils") + native int strlen_safe(byte[])
    //         -> cpp-kit edge (non-rust, non-system lib -> cpp-kit)
    // -----------------------------------------------------------------------

    @Test
    public void test2_cLibrary_nativeMethod_emitsCppKitEdge() {
        String source = """
            public class StringUtils {
                static {
                    System.loadLibrary("c_string_utils");
                }
                public native int strlen_safe(byte[] buf);
                public int computeLen(byte[] buf) {
                    return strlen_safe(buf);
                }
            }
            """;

        Map<String, String> index = contractIndex("StringUtils.computeLen");
        List<CallEdgeDecl> edges = resolve(source, index);

        assertEquals(1, edges.size());
        CallEdgeDecl edge = edges.get(0);

        assertEquals("cpp-kit:StringUtils.strlen_safe", edge.targetSymbol,
            "Non-rust, non-system library -> cpp-kit");
        assertNull(edge.targetContractCid);
    }

    // -----------------------------------------------------------------------
    // Test 3: Class with no native methods -> no JNI call-edges
    // -----------------------------------------------------------------------

    @Test
    public void test3_noNativeMethods_noEdges() {
        String source = """
            public class PureJava {
                public int add(int a, int b) {
                    return a + b;
                }
                public int multiply(int a, int b) {
                    return a * b;
                }
            }
            """;

        Map<String, String> index = contractIndex("PureJava.add", "PureJava.multiply");
        List<CallEdgeDecl> edges = resolve(source, index);

        assertTrue(edges.isEmpty(), "No native methods -> no JNI call-edges");
    }

    // -----------------------------------------------------------------------
    // Test 4: Unknown library (no System.loadLibrary) -> resolver-error, NOT placeholder
    // -----------------------------------------------------------------------

    @Test
    public void test4_unknownLibrary_emitsResolverErrorNotPlaceholder() {
        // Native method declared with no loadLibrary -> kit is unknown.
        String source = """
            public class Foo {
                public native void bar(int n);
                public void caller(int n) {
                    bar(n);
                }
            }
            """;

        Map<String, String> index = contractIndex("Foo.caller");
        List<CallEdgeDecl> edges = resolve(source, index);

        assertEquals(1, edges.size());
        CallEdgeDecl edge = edges.get(0);

        assertTrue(edge.targetSymbol.startsWith("resolver-error:"),
            "Unknown library must use resolver-error prefix, got: " + edge.targetSymbol);
        assertFalse(edge.targetSymbol.contains("pending"),
            "Must NOT use placeholder strings");
        assertFalse(edge.targetSymbol.contains("unknown"),
            "Must use resolver-error prefix specifically");
    }

    // -----------------------------------------------------------------------
    // Test 5: Byte-determinism — two runs over same source produce identical output
    // -----------------------------------------------------------------------

    @Test
    public void test5_byteDeterminism_twoRunsProduceIdenticalOutput() {
        String source = """
            public class RustBindings {
                static {
                    System.loadLibrary("rust_callee");
                }
                public native int process(int n);
                public int caller(int value) {
                    return process(value);
                }
            }
            """;

        Map<String, String> index = contractIndex("RustBindings.caller");

        List<CallEdgeDecl> edges1 = resolve(source, index);
        List<CallEdgeDecl> edges2 = resolve(source, index);

        assertEquals(edges1.size(), edges2.size(), "Edge count must be identical across runs");
        for (int i = 0; i < edges1.size(); i++) {
            assertEquals(edges1.get(i).toJson(), edges2.get(i).toJson(),
                "Call-edge JSON at index " + i + " must be byte-identical across runs");
        }
    }

    // -----------------------------------------------------------------------
    // Test 6: System.load("/absolute/path/libfoo.so") -> extracts "foo"
    //         Pattern B: absolute path form
    // -----------------------------------------------------------------------

    @Test
    public void test6_systemLoad_absolutePath_extractsLibName() {
        String source = """
            public class Foo {
                static {
                    System.load("/usr/local/lib/librust_callee.so");
                }
                public native void bar(int n);
                public void caller(int n) {
                    bar(n);
                }
            }
            """;

        Map<String, String> index = contractIndex("Foo.caller");
        List<CallEdgeDecl> edges = resolve(source, index);

        assertEquals(1, edges.size());
        assertEquals("rust-kit:Foo.bar", edges.get(0).targetSymbol,
            "System.load with absolute path must strip lib prefix and .so suffix");
    }

    // -----------------------------------------------------------------------
    // Test 7: System library (libc) -> libc-system pseudo-kit
    // -----------------------------------------------------------------------

    @Test
    public void test7_systemLib_emitsLibcSystemEdge() {
        String source = """
            public class LibcBindings {
                static {
                    System.loadLibrary("c");
                }
                public native int getpid();
                public int caller() {
                    return getpid();
                }
            }
            """;

        Map<String, String> index = contractIndex("LibcBindings.caller");
        List<CallEdgeDecl> edges = resolve(source, index);

        assertEquals(1, edges.size());
        assertEquals("libc-system:LibcBindings.getpid", edges.get(0).targetSymbol,
            "System library must map to libc-system pseudo-kit");
    }

    // -----------------------------------------------------------------------
    // Utility tests: stripLibName and resolveKit
    // -----------------------------------------------------------------------

    @Test
    public void testStripLibName() {
        assertEquals("rust_callee", JniResolver.stripLibName("rust_callee"));
        assertEquals("rust_callee", JniResolver.stripLibName("librust_callee.so"));
        assertEquals("rust_callee", JniResolver.stripLibName("/usr/local/lib/librust_callee.so"));
        assertEquals("foo",         JniResolver.stripLibName("foo.dll"));
        assertEquals("foo",         JniResolver.stripLibName("libfoo.dylib"));
        assertEquals("c",           JniResolver.stripLibName("libc.so.6"));
        assertEquals("",            JniResolver.stripLibName(""));
        assertEquals("",            JniResolver.stripLibName(null));
    }

    @Test
    public void testResolveKit() {
        assertEquals("rust-kit",    JniResolver.resolveKit("rust_callee"));
        assertEquals("rust-kit",    JniResolver.resolveKit("rustfoo"));
        assertEquals("libc-system", JniResolver.resolveKit("c"));
        assertEquals("libc-system", JniResolver.resolveKit("m"));
        assertEquals("libc-system", JniResolver.resolveKit("pthread"));
        assertEquals("libc-system", JniResolver.resolveKit("ssl"));
        assertEquals("cpp-kit",     JniResolver.resolveKit("c_string_utils"));
        assertEquals("cpp-kit",     JniResolver.resolveKit("foo"));
        assertNull(                  JniResolver.resolveKit(""));
        assertNull(                  JniResolver.resolveKit(null));
    }
}
