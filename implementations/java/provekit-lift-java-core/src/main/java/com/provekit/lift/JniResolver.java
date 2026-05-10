package com.provekit.lift;

import java.util.*;
import com.github.javaparser.*;
import com.github.javaparser.ast.*;
import com.github.javaparser.ast.body.*;
import com.github.javaparser.ast.expr.*;
import com.github.javaparser.ast.stmt.*;
import com.github.javaparser.ast.visitor.VoidVisitorAdapter;
import com.provekit.ir.CallEdgeDecl;
import com.provekit.ir.Formula;
import com.provekit.ir.Sort;
import com.provekit.ir.Term;

/**
 * JNI FFI call-site resolver per spec #114 R3.
 *
 * Walks a Java CompilationUnit and detects JNI FFI patterns:
 *   - System.loadLibrary("name") or System.load("/path/to/lib") in static
 *     initializers: extracts the library name.
 *   - native method declarations: registers class.method -> (kit, symbol).
 *   - Method call sites that invoke native methods: emits CallEdgeDecl
 *     mementos with targetContractCid = null and
 *     targetSymbol = "<kit>:<ClassName>.<methodName>".
 *
 * Library-name-to-kit resolution mirrors Go's resolveCgoKit:
 *   - rust* prefix  -> rust-kit
 *   - system libs   -> libc-system (opaque pseudo-kit)
 *   - any other lib -> cpp-kit
 *   - empty/unknown -> resolver-error:<symbol> (fail-loud per spec #97 R2)
 *
 * Mirrors Go's cgo resolver in
 * implementations/go/cmd/provekit-lsp-go/main.go (PR #127) and
 * Python's ctypes resolver (PR #131).
 */
public class JniResolver {

    // System libraries that map to libc-system (opaque to the linker).
    private static final Set<String> SYSTEM_LIBS = new HashSet<>(Arrays.asList(
        "c", "m", "pthread", "dl", "rt", "util", "resolv", "nsl",
        "z", "bz2", "lzma", "crypto", "ssl", "curl"
    ));

    /**
     * Resolve a normalised library name to a kit prefix.
     *
     * Resolution order (matches Go resolveCgoKit):
     *   1. "rust" prefix (case-insensitive)  -> "rust-kit"
     *   2. System libs                        -> "libc-system"
     *   3. Any other non-empty name           -> "cpp-kit"
     *   4. Empty name                         -> null (caller emits resolver-error)
     */
    static String resolveKit(String libName) {
        if (libName == null || libName.isEmpty()) {
            return null;
        }
        if (libName.toLowerCase(java.util.Locale.ROOT).startsWith("rust")) {
            return "rust-kit";
        }
        if (SYSTEM_LIBS.contains(libName)) {
            return "libc-system";
        }
        // Any other explicit lib name -> cpp-kit.
        return "cpp-kit";
    }

    /**
     * Strip lib prefix and .so/.dll/.dylib suffix from a path or library name.
     *
     * Examples:
     *   "/usr/local/lib/librust_callee.so" -> "rust_callee"
     *   "rust_callee"                      -> "rust_callee"
     *   "libfoo"                           -> "foo"
     *   "foo.dll"                          -> "foo"
     *   "libc.so.6"                        -> "c"
     */
    static String stripLibName(String raw) {
        if (raw == null || raw.isEmpty()) return "";
        // Basename only.
        int slash = Math.max(raw.lastIndexOf('/'), raw.lastIndexOf('\\'));
        String name = slash >= 0 ? raw.substring(slash + 1) : raw;

        // Strip known extensions repeatedly (handles libc.so.6).
        while (true) {
            int dot = name.lastIndexOf('.');
            if (dot < 0) break;
            String ext = name.substring(dot).toLowerCase(java.util.Locale.ROOT);
            if (ext.equals(".so") || ext.equals(".dll") || ext.equals(".dylib") || ext.equals(".a")) {
                name = name.substring(0, dot);
            } else {
                // Trailing version component like .6
                boolean allDigits = true;
                for (int i = dot + 1; i < name.length(); i++) {
                    if (!Character.isDigit(name.charAt(i))) { allDigits = false; break; }
                }
                if (allDigits && dot + 1 < name.length()) {
                    name = name.substring(0, dot);
                } else {
                    break;
                }
            }
        }

        // Strip 'lib' prefix.
        if (name.startsWith("lib") && name.length() > 3) {
            name = name.substring(3);
        }

        return name;
    }

    /**
     * Scan a CompilationUnit for JNI call-edge patterns and emit mementos.
     *
     * @param cu         parsed CompilationUnit
     * @param path       source file path (for locus)
     * @param contractIndex map from "ClassName.methodName" -> contractCid for known contracts
     * @return list of CallEdgeDecl mementos
     */
    public static List<CallEdgeDecl> resolve(
            CompilationUnit cu,
            String path,
            Map<String, String> contractIndex) {

        List<CallEdgeDecl> edges = new ArrayList<>();
        if (contractIndex == null || contractIndex.isEmpty()) {
            return edges;
        }

        // Walk each class declaration.
        cu.findAll(ClassOrInterfaceDeclaration.class).forEach(cls -> {
            String className = cls.getNameAsString();

            // Step 1: find library load in static initializers.
            String libName = extractLoadedLibrary(cls);
            String kit = (libName != null) ? resolveKit(libName) : null;

            // Step 2: collect native method declarations.
            // Map: methodName -> (kit, targetSymbol)
            Map<String, String> nativeMethods = new HashMap<>();
            for (MethodDeclaration method : cls.getMethods()) {
                if (method.isNative()) {
                    String sym;
                    if (kit != null) {
                        sym = kit + ":" + className + "." + method.getNameAsString();
                    } else {
                        // Unknown library -> resolver-error per spec #97 R2.
                        sym = "resolver-error:" + className + "." + method.getNameAsString();
                    }
                    nativeMethods.put(method.getNameAsString(), sym);
                }
            }

            if (nativeMethods.isEmpty()) {
                return; // No native methods -> no JNI call edges.
            }

            // Step 3: find call sites for native methods in non-native method bodies.
            for (MethodDeclaration callerMethod : cls.getMethods()) {
                if (callerMethod.isNative() || callerMethod.getBody().isEmpty()) {
                    continue;
                }
                String callerSymbol = className + "." + callerMethod.getNameAsString();
                String sourceCid = contractIndex.get(callerSymbol);
                if (sourceCid == null) {
                    // Caller has no contract; skip per R1.
                    continue;
                }

                callerMethod.getBody().get().findAll(MethodCallExpr.class).forEach(call -> {
                    String calleeName = call.getNameAsString();
                    if (!nativeMethods.containsKey(calleeName)) {
                        return;
                    }
                    String targetSymbol = nativeMethods.get(calleeName);

                    int line = call.getBegin().map(p -> p.line).orElse(0);
                    int column = call.getBegin().map(p -> p.column).orElse(0);

                    // Build evidence term: atomic("call-site-obligation", var(callerSymbol, String)).
                    Formula evidenceTerm = Formula.atomic(
                        "call-site-obligation",
                        Term.var_(callerSymbol, Sort.String)
                    );

                    edges.add(new CallEdgeDecl(
                        sourceCid,
                        null,           // cross-kit: targetContractCid = null
                        targetSymbol,
                        path,
                        line,
                        column,
                        evidenceTerm.toJson()
                    ));
                });
            }
        });

        return edges;
    }

    /**
     * Extract the library name from System.loadLibrary("name") or
     * System.load("/path/to/lib") calls within static initializers of a class.
     *
     * Returns the normalised library name (stripped of lib prefix and
     * .so/.dll/.dylib suffix), or null if no load was found.
     */
    static String extractLoadedLibrary(ClassOrInterfaceDeclaration cls) {
        for (InitializerDeclaration init : cls.findAll(InitializerDeclaration.class)) {
            if (!init.isStatic()) continue;
            for (MethodCallExpr call : init.findAll(MethodCallExpr.class)) {
                String name = call.getNameAsString();
                if ((name.equals("loadLibrary") || name.equals("load"))
                        && call.getScope().map(s -> s.toString().equals("System")).orElse(false)
                        && call.getArguments().size() == 1) {
                    Expression arg = call.getArgument(0);
                    if (arg instanceof StringLiteralExpr) {
                        String raw = ((StringLiteralExpr) arg).asString();
                        return stripLibName(raw);
                    }
                }
            }
        }
        return null;
    }
}
