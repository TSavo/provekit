// SPDX-License-Identifier: Apache-2.0
//
// LSPTests: standalone integration test runner for provekit-lsp-swift.
//
// Does NOT depend on XCTest or Swift Testing (neither is available
// without full Xcode on CI). Uses Foundation Process + Pipe to spawn
// the binary and assert response shapes. Exits 0 on all pass, 1 on any fail.
//
// Mirrors the pattern of:
//   implementations/python/provekit-lift-py-tests/tests/test_daemon_protocol.py
//   implementations/swift/Sources/ConformanceRunner/main.swift (PASS/FAIL idiom)

import Foundation
import SwiftLifter

// MARK: - Test framework

nonisolated(unsafe) var passed = 0
nonisolated(unsafe) var skipped = 0
nonisolated(unsafe) var failed = 0

func test(_ name: String, block: () -> Bool) {
    if block() {
        print("PASS: \(name)")
        passed += 1
    } else {
        print("FAIL: \(name)")
        failed += 1
    }
}

func assert(_ condition: Bool, _ message: String = "") -> Bool {
    if !condition && !message.isEmpty {
        print("  assertion failed: \(message)")
    }
    return condition
}

func jsonEscaped(_ value: String) -> String {
    var out = ""
    for scalar in value.unicodeScalars {
        switch scalar.value {
        case 0x22: out += "\\\""
        case 0x5C: out += "\\\\"
        case 0x08: out += "\\b"
        case 0x0C: out += "\\f"
        case 0x0A: out += "\\n"
        case 0x0D: out += "\\r"
        case 0x09: out += "\\t"
        case 0..<0x20:
            out += String(format: "\\u%04x", scalar.value)
        default:
            out.unicodeScalars.append(scalar)
        }
    }
    return out
}

// MARK: - Unit tests for SwiftLifter (no subprocess)

test("lifter extracts 2 func declarations") {
    let source = """
    func greet(name: String) -> String {
        return "Hello"
    }
    func farewell(name: String) -> String {
        return "Goodbye"
    }
    """
    let result = SwiftLifter.lift(source: source, path: "test.swift")
    return assert(result.declarations.count == 2,
        "Expected 2, got \(result.declarations.count)")
}

test("lifter extracts correct function names") {
    let source = """
    func alpha() {}
    func beta() {}
    """
    let result = SwiftLifter.lift(source: source, path: "test.swift")
    let names = result.declarations.map { $0.name }
    return assert(names.contains("alpha") && names.contains("beta"),
        "Expected alpha and beta, got \(names)")
}

test("lifter extracts call edge from fixture (2 funcs, 1 call)") {
    let source = """
    func add(a: Int, b: Int) -> Int {
        return a + b
    }
    func compute() -> Int {
        return add(a: 1, b: 2)
    }
    """
    let result = SwiftLifter.lift(source: source, path: "fixture.swift")
    let declOk = result.declarations.count == 2
    let edgeOk = result.callEdges.count >= 1
    let symbolOk = result.callEdges.contains {
        $0.sourceContractCid == "pending-swift:compute" &&
        $0.targetSymbol == "swift-kit:add"
    }
    return assert(declOk, "Expected 2 decls, got \(result.declarations.count)") &&
           assert(edgeOk, "Expected >=1 call edge, got \(result.callEdges.count)") &&
           assert(symbolOk, "Expected call edge from compute to swift-kit:add")
}

test("declaration wire shape has correct fields") {
    let source = "func myFunc(x: Int) -> Int { return x }"
    let result = SwiftLifter.lift(source: source, path: "t.swift")
    guard let decl = result.declarations.first else {
        print("  no declarations")
        return false
    }
    return assert(decl.kind == "contract", "kind=\(decl.kind)") &&
           assert(decl.name == "myFunc", "name=\(decl.name)") &&
           assert(decl.outBinding == "out", "outBinding=\(decl.outBinding)")
}

test("empty source produces empty result") {
    let result = SwiftLifter.lift(source: "", path: "empty.swift")
    return assert(result.declarations.isEmpty, "Expected no decls") &&
           assert(result.callEdges.isEmpty, "Expected no edges")
}

test("call edge locus file matches path") {
    let source = """
    func alpha() {}
    func beta() { alpha() }
    """
    let result = SwiftLifter.lift(source: source, path: "/proj/foo.swift")
    guard let edge = result.callEdges.first else {
        print("  no call edges")
        return false
    }
    return assert(edge.callSiteLocus.file == "/proj/foo.swift",
        "file=\(edge.callSiteLocus.file)")
}

test("declaration toDict has correct shape") {
    let d = LiftedDeclaration(name: "foo")
    let dict = d.toDict()
    return assert(dict["kind"] as? String == "contract") &&
           assert(dict["name"] as? String == "foo") &&
           assert(dict["outBinding"] as? String == "out")
}

test("call edge toDict has correct shape") {
    let e = LiftedCallEdge(
        sourceContractCid: "",
        targetSymbol: "bar",
        callSiteLocus: CallSiteLocus(file: "f.swift", line: 5, column: 3)
    )
    let dict = e.toDict()
    let locus = dict["callSiteLocus"] as? [String: Any]
    return assert(dict["targetSymbol"] as? String == "bar") &&
           assert(locus != nil) &&
           assert(locus?["file"] as? String == "f.swift") &&
           assert(locus?["line"] as? Int == 5) &&
           assert(locus?["column"] as? Int == 3)
}

// MARK: - Subprocess integration test

/// Finds the provekit-lsp-swift binary in .build/debug or .build/release.
func findLSPBinary() -> String? {
    // The executable is in the same .build tree as this test binary.
    // When run via `swift run test-swift-lsp`, the CWD is the package root.
    let candidates: [String] = [
        ".build/debug/provekit-lsp-swift",
        ".build/release/provekit-lsp-swift",
    ]
    let fm = FileManager.default
    let cwd = fm.currentDirectoryPath
    for rel in candidates {
        let full = cwd + "/" + rel
        if fm.fileExists(atPath: full) {
            return full
        }
    }
    return nil
}

/// Send one NDJSON line and wait for one response line.
/// Returns parsed JSON dictionary or nil on timeout/error.
func exchange(writeHandle: FileHandle, readHandle: FileHandle, readBuffer: inout Data, _ json: String) -> [String: Any]? {
    let line = json + "\n"
    guard let data = line.data(using: .utf8) else { return nil }
    writeHandle.write(data)

    let deadline = Date().addingTimeInterval(10)
    while Date() < deadline {
        let chunk = readHandle.availableData
        if !chunk.isEmpty {
            readBuffer.append(chunk)
        }
        if let nlRange = readBuffer.range(of: Data([0x0A])) {
            let lineData = readBuffer[readBuffer.startIndex..<nlRange.upperBound]
            readBuffer = Data(readBuffer[nlRange.upperBound...])
            if let parsed = try? JSONSerialization.jsonObject(with: lineData) as? [String: Any] {
                return parsed
            }
        }
        Thread.sleep(forTimeInterval: 0.05)
    }
    return nil
}

if let binaryPath = findLSPBinary() {
    test("subprocess: initialize returns correct shape") {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: binaryPath)
        let stdinPipe = Pipe()
        let stdoutPipe = Pipe()
        process.standardInput = stdinPipe
        process.standardOutput = stdoutPipe
        process.standardError = FileHandle.nullDevice

        do { try process.run() } catch {
            print("  failed to spawn: \(error)")
            return false
        }

        var buf = Data()
        let resp = exchange(
            writeHandle: stdinPipe.fileHandleForWriting,
            readHandle: stdoutPipe.fileHandleForReading,
            readBuffer: &buf,
            #"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#
        )

        // Send shutdown to clean up.
        stdinPipe.fileHandleForWriting.write(
            (#"{"jsonrpc":"2.0","id":2,"method":"shutdown"}"# + "\n").data(using: .utf8)!
        )
        process.waitUntilExit()

        guard let r = resp, let result = r["result"] as? [String: Any] else {
            print("  no result in response: \(String(describing: resp))")
            return false
        }
        let capabilities = result["capabilities"] as? [String: Any]
        let sourceSurfaces = capabilities?["source_surfaces"] as? [String]
        let diagnosticCodes = capabilities?["diagnostic_codes"] as? [String]

        return assert(result["name"] as? String == "provekit-lsp-swift", "name=\(String(describing: result["name"]))") &&
               assert(result["version"] as? String == "0.1.0", "version=\(String(describing: result["version"]))") &&
               assert(result["protocol_version"] as? String == "provekit-lsp-shared/1", "protocol=\(String(describing: result["protocol_version"]))") &&
               assert(result["kit_id"] as? String == "swift", "kit_id=\(String(describing: result["kit_id"]))") &&
               assert((result["protocol_catalog_cid"] as? String ?? "").hasPrefix("blake3-512:"), "protocol_catalog_cid=\(String(describing: result["protocol_catalog_cid"]))") &&
               assert(sourceSurfaces == ["swift-source"], "source_surfaces=\(String(describing: sourceSurfaces))") &&
               assert(diagnosticCodes?.contains("provekit.lsp.implication_failed") == true, "diagnostic_codes=\(String(describing: diagnosticCodes))")
    }

    test("subprocess: parse returns declarations and callEdges as arrays") {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: binaryPath)
        let stdinPipe = Pipe()
        let stdoutPipe = Pipe()
        process.standardInput = stdinPipe
        process.standardOutput = stdoutPipe
        process.standardError = FileHandle.nullDevice

        do { try process.run() } catch {
            print("  failed to spawn: \(error)")
            return false
        }

        var buf = Data()
        let wh = stdinPipe.fileHandleForWriting
        let rh = stdoutPipe.fileHandleForReading

        // initialize first
        _ = exchange(writeHandle: wh, readHandle: rh, readBuffer: &buf,
            #"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#)

        // parse fixture: 2 funcs, 1 call edge
        let fixtureSource = "func add(a: Int, b: Int) -> Int { return a + b }\\nfunc compute() -> Int { return add(a: 1, b: 2) }"
        let parseReq = #"{"jsonrpc":"2.0","id":2,"method":"parse","params":{"path":"fixture.swift","source":""# + fixtureSource + #""}}"#
        let parseResp = exchange(writeHandle: wh, readHandle: rh, readBuffer: &buf, parseReq)

        // shutdown
        wh.write((#"{"jsonrpc":"2.0","id":3,"method":"shutdown"}"# + "\n").data(using: .utf8)!)
        process.waitUntilExit()

        guard let pr = parseResp, let result = pr["result"] as? [String: Any] else {
            print("  no result in parse response: \(String(describing: parseResp))")
            return false
        }

        let decls = result["declarations"] as? [[String: Any]]
        let edges = result["callEdges"] as? [[String: Any]]
        let warnings = result["warnings"]
        let edgeOk = edges?.contains {
            $0["sourceContractCid"] as? String == "pending-swift:compute" &&
            $0["targetSymbol"] as? String == "swift-kit:add"
        } ?? false

        return assert(decls != nil, "declarations not an array of objects") &&
               assert((decls?.count ?? 0) == 2, "Expected 2 decls, got \(decls?.count ?? -1)") &&
               assert(edges != nil, "callEdges not an array of objects") &&
               assert((edges?.count ?? 0) >= 1, "Expected >=1 call edge, got \(edges?.count ?? -1)") &&
               assert(edgeOk, "Expected call edge from compute to swift-kit:add") &&
               assert(warnings != nil, "warnings field missing")
    }

    test("subprocess: analyzeDocument returns callsite diagnostic") {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: binaryPath)
        let stdinPipe = Pipe()
        let stdoutPipe = Pipe()
        process.standardInput = stdinPipe
        process.standardOutput = stdoutPipe
        process.standardError = FileHandle.nullDevice

        do { try process.run() } catch {
            print("  failed to spawn: \(error)")
            return false
        }

        var buf = Data()
        let wh = stdinPipe.fileHandleForWriting
        let rh = stdoutPipe.fileHandleForReading

        _ = exchange(writeHandle: wh, readHandle: rh, readBuffer: &buf,
            #"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#)

        let source = """
        // Forward-propagation floor fixture for Swift
        // Tests: (1) callsite satisfies pre, no diagnostic | (2) callsite violates pre, diagnostic | (3) loop path, top fallback

        func checkPositive(_ x: Int) -> Bool {
            if x <= 0 { return false }  // pre: x > 0
            return true
        }

        func callerSatisfiesPre() -> Bool {
            let result = checkPositive(5)  // satisfies pre (x=5 > 0)
            return result
        }

        func callerViolatesPre() -> Bool {
            let result = checkPositive(-1)  // violates pre (x=-1 <= 0)
            return result
        }

        func callerWithLoop() -> Bool {
            for i in 0..<10 {
                let result = checkPositive(i)  // top fallback at loop entry
                if !result { return false }
            }
            return true
        }
        """
        let request = #"{"jsonrpc":"2.0","id":2,"method":"analyzeDocument","params":{"kit_id":"swift","uri":"file:///project/FloorFixture.swift","file":"FloorFixture.swift","text":""# +
            jsonEscaped(source) +
            #"","document_version":42,"workspace_root":"/project","accepted_protocol_catalog_cids":[],"policy_cids":[]}}"#
        let resp = exchange(writeHandle: wh, readHandle: rh, readBuffer: &buf, request)

        wh.write((#"{"jsonrpc":"2.0","id":3,"method":"shutdown"}"# + "\n").data(using: .utf8)!)
        process.waitUntilExit()

        guard let r = resp, let result = r["result"] as? [String: Any] else {
            print("  no result in analyze response: \(String(describing: resp))")
            return false
        }

        let diagnostics = result["diagnostics"] as? [[String: Any]]
        let diagnostic = diagnostics?.first
        let data = diagnostic?["data"] as? [String: Any]
        let range = diagnostic?["range"] as? [String: Any]

        return assert(result["kind"] as? String == "lsp-document-analysis", "kind=\(String(describing: result["kind"]))") &&
               assert(result["kit_id"] as? String == "swift", "kit_id=\(String(describing: result["kit_id"]))") &&
               assert((result["document_cid"] as? String ?? "").hasPrefix("blake3-512:"), "document_cid=\(String(describing: result["document_cid"]))") &&
               assert(diagnostics?.count == 1, "diagnostics=\(String(describing: diagnostics))") &&
               assert(diagnostic?["code"] as? String == "provekit.lsp.implication_failed", "code=\(String(describing: diagnostic?["code"]))") &&
               assert(diagnostic?["severity"] as? String == "error", "severity=\(String(describing: diagnostic?["severity"]))") &&
               assert(diagnostic?["producer"] as? String == "forward-propagation", "producer=\(String(describing: diagnostic?["producer"]))") &&
               assert(data?["callee"] as? String == "checkPositive", "callee=\(String(describing: data?["callee"]))") &&
               assert(range?["start_line"] as? Int == 15, "start_line=\(String(describing: range?["start_line"]))") &&
               assert(range?["start_col"] as? Int == 17, "start_col=\(String(describing: range?["start_col"]))")
    }

    test("subprocess: unknown method returns error") {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: binaryPath)
        let stdinPipe = Pipe()
        let stdoutPipe = Pipe()
        process.standardInput = stdinPipe
        process.standardOutput = stdoutPipe
        process.standardError = FileHandle.nullDevice

        do { try process.run() } catch { return false }

        var buf = Data()
        let resp = exchange(
            writeHandle: stdinPipe.fileHandleForWriting,
            readHandle: stdoutPipe.fileHandleForReading,
            readBuffer: &buf,
            #"{"jsonrpc":"2.0","id":1,"method":"frobnicate","params":{}}"#
        )
        stdinPipe.fileHandleForWriting.write(
            (#"{"jsonrpc":"2.0","id":2,"method":"shutdown"}"# + "\n").data(using: .utf8)!
        )
        process.waitUntilExit()

        guard let r = resp else { return false }
        return assert(r["error"] != nil, "Expected error for unknown method")
    }
} else {
    print("SKIP: subprocess tests (binary not found; run `swift build` first, then `swift run test-swift-lsp`)")
    skipped += 1
}

// MARK: - Result

print("")
print("Results: \(passed) passed, \(skipped) skipped, \(failed) failed")
if failed > 0 {
    exit(1)
}
