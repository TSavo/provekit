// SPDX-License-Identifier: Apache-2.0
//
// LSPTests — standalone integration test runner for provekit-lsp-swift.
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
    let symbolOk = result.callEdges.contains { $0.targetSymbol == "add" }
    return assert(declOk, "Expected 2 decls, got \(result.declarations.count)") &&
           assert(edgeOk, "Expected >=1 call edge, got \(result.callEdges.count)") &&
           assert(symbolOk, "Expected call edge to 'add'")
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
        return assert(result["name"] as? String == "provekit-lsp-swift", "name=\(String(describing: result["name"]))") &&
               assert(result["version"] as? String == "0.1.0", "version=\(String(describing: result["version"]))") &&
               assert((result["capabilities"] as? [String]) == ["parse"], "caps=\(String(describing: result["capabilities"]))")
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

        return assert(decls != nil, "declarations not an array of objects") &&
               assert((decls?.count ?? 0) == 2, "Expected 2 decls, got \(decls?.count ?? -1)") &&
               assert(edges != nil, "callEdges not an array of objects") &&
               assert((edges?.count ?? 0) >= 1, "Expected >=1 call edge, got \(edges?.count ?? -1)") &&
               assert(warnings != nil, "warnings field missing")
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
