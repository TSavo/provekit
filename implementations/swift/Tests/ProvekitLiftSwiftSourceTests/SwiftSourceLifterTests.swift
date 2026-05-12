import Foundation
import XCTest
import ProvekitCrypto
import ProvekitLiftSwiftSource

final class SwiftSourceLifterTests: XCTestCase {
    func testLiftFunctionEmitsSourceUnitAndSwiftOps() throws {
        let source = """
        var GLOBAL = 3

        func addOne(_ x: Int) -> Int {
            let y = x + GLOBAL
            return y
        }
        """

        let result = SwiftSourceLifter.liftSource(source, path: "pkg/Mod.swift")

        XCTAssertEqual(result.refusals, [])
        XCTAssertEqual(result.ir.map { SwiftSourceIR.fnName(of: $0) }, [
            "<source-unit:pkg/Mod.swift>",
            "pkg.Mod.addOne(_:)(Int)->Int",
        ])

        let sourceUnit = try SwiftSourceIR.bodyTerm(of: result.ir[0])
        let sourceUnitJson = try JSONObject(sourceUnit)
        XCTAssertEqual(sourceUnitJson["name"] as? String, "swift:source-unit")
        let sourceArgs = try XCTUnwrap(sourceUnitJson["args"] as? [[String: Any]])
        XCTAssertEqual(sourceArgs[0]["value"] as? String, source)

        let contract = try XCTUnwrap(result.ir.first { SwiftSourceIR.fnName(of: $0).hasSuffix(".addOne(_:)(Int)->Int") })
        let contractJson = try JSONObject(contract)
        XCTAssertEqual(contractJson["formals"] as? [String], ["x"])
        XCTAssertEqual(contractJson["effects"] as? [[String: String]], [
            ["kind": "reads", "target": "GLOBAL"],
        ])

        let body = try SwiftSourceIR.bodyTerm(of: contract)
        XCTAssertEqual(ctorNames(try JSONValue(body)), [
            "swift:seq",
            "swift:assign",
            "swift:add",
            "swift:return",
        ])
        XCTAssertFalse(try canonicalString(result.ir).contains("swift:unknown"))
        XCTAssertFalse(try canonicalString(result.ir).contains("swift:skip"))
    }

    func testRefusesUnhandledSyntaxWithoutUnknownOps() throws {
        let source = """
        func bad(_ x: Int) -> Int {
            let f = { x + 1 }
            return f()
        }
        """

        let result = SwiftSourceLifter.liftSource(source, path: "Bad.swift")

        XCTAssertEqual(result.ir.count, 1)
        XCTAssertEqual(SwiftSourceIR.fnName(of: result.ir[0]), "<source-unit:Bad.swift>")
        let sourceUnit = try SwiftSourceIR.bodyTerm(of: result.ir[0])
        let sourceUnitJson = try JSONObject(sourceUnit)
        XCTAssertEqual(sourceUnitJson["name"] as? String, "swift:source-unit")
        let sourceArgs = try XCTUnwrap(sourceUnitJson["args"] as? [[String: Any]])
        XCTAssertEqual(sourceArgs[1]["name"] as? String, "swift:empty")

        XCTAssertEqual(result.refusals.count, 1)
        let refusal = try JSONObject(result.refusals[0])
        XCTAssertEqual(refusal["kind"] as? String, "unhandled-syntax")
        XCTAssertEqual(refusal["function"] as? String, "Bad.bad(_:)(Int)->Int")
        XCTAssertEqual(refusal["line"] as? Int, 2)
        XCTAssertTrue(String(describing: refusal["reason"] ?? "").contains("ClosureExprSyntax"))
        XCTAssertFalse(try canonicalString(result.refusals).contains("swift:unknown"))
        XCTAssertFalse(try canonicalString(result.refusals).contains("swift:skip"))
    }

    func testRefusesUnparseableFunctionSignatureNotSilentlyDropped() throws {
        // Functions whose signatures fail validateSignature must produce a
        // Refusal, not be silently omitted from the output.
        let source = """
        func good(_ x: Int) -> Int {
            return x + 1
        }

        func generic<T>(_ x: T) -> T {
            return x
        }

        func asyncFunc(_ x: Int) async -> Int {
            return x
        }
        """

        let result = SwiftSourceLifter.liftSource(source, path: "Sig.swift")

        // The good function produces a contract; the two unsupported ones produce refusals.
        let contractNames = result.ir.compactMap { SwiftSourceIR.fnName(of: $0) }
        XCTAssertTrue(contractNames.contains { $0.contains(".good(_:)(Int)->Int") },
                      "good function must be lifted: \(contractNames)")

        // Each unsupported function must yield a refusal, not be invisible.
        XCTAssertGreaterThanOrEqual(result.refusals.count, 2,
            "generic and async functions must produce refusals, got \(result.refusals.count)")

        // Refusals must carry the function name (best-effort) and a reason.
        for refusal in result.refusals {
            let obj = try JSONObject(refusal)
            XCTAssertNotNil(obj["kind"],   "refusal must have a kind")
            XCTAssertNotNil(obj["reason"], "refusal must have a reason")
        }

        XCTAssertFalse(try canonicalString(result.refusals).contains("swift:unknown"))
        XCTAssertFalse(try canonicalString(result.refusals).contains("swift:skip"))
    }

    func testEffectsAreSortedAndLoopCidIsBlake3512() throws {
        let source = """
        func total(_ limit: Int) -> Int {
            var acc = 0
            while acc < limit {
                acc = acc + 1
            }
            print(acc)
            return acc
        }
        """

        let result = SwiftSourceLifter.liftSource(source, path: "Loops.swift")

        let contract = try XCTUnwrap(result.ir.first { SwiftSourceIR.fnName(of: $0).contains(".total(_:)(Int)->Int") })
        let contractJson = try JSONObject(contract)
        let effects = try XCTUnwrap(contractJson["effects"] as? [[String: Any]])
        XCTAssertEqual(effects.map { $0["kind"] as? String }, ["io", "opaque_loop"])
        let loopCid = try XCTUnwrap(effects[1]["loopCid"] as? String)
        XCTAssertTrue(loopCid.hasPrefix("blake3-512:"))
        XCTAssertEqual(loopCid.count, "blake3-512:".count + 128)
    }

    func testCompileLiftRoundtripBodyTermIsByteIdentical() throws {
        let source = """
        func f(_ x: Int) -> Int {
            let y = x + 1
            return y
        }
        """
        let lifted = SwiftSourceLifter.liftSource(source, path: "RoundTrip.swift")
        let contract = try XCTUnwrap(lifted.ir.first { SwiftSourceIR.fnName(of: $0).contains(".f(_:)(Int)->Int") })
        let body = try SwiftSourceIR.bodyTerm(of: contract)

        let compiled = try SwiftSourceCompiler.compileBodyTerm(
            body,
            fnName: "f",
            formals: ["x"]
        )
        let relifted = SwiftSourceLifter.liftSource(compiled, path: "RoundTrip.swift")
        let reliftedContract = try XCTUnwrap(relifted.ir.first { SwiftSourceIR.fnName(of: $0).contains(".f(_:)(Int)->Int") })
        let reliftedBody = try SwiftSourceIR.bodyTerm(of: reliftedContract)

        XCTAssertEqual(
            JcsCanonicalizer.encode(reliftedBody),
            JcsCanonicalizer.encode(body)
        )
    }

    func testCanonicalCidUsesExistingJcsAndBlake3Implementation() {
        let value = JcsCanonical.object([("source", .string("func f() -> Int {\n    return 1\n}\n"))])
        let expected = "blake3-512:fd24d936f4813306e1f3fd0c0128a7444539be58a089ace9f856f42c8b34d6cc97d9c9b924445e0b91c5843fe90198c6a82d754d79ee20a2582fa2655e3d05f3"

        XCTAssertEqual(computeJcsCid(value), expected)
    }

    func testRpcInitializeDeclaresSwiftSourceDraft() {
        let result = SwiftSourceRPC.initializeResult()

        XCTAssertEqual(result["version"] as? String, "0.1.0-draft")
        XCTAssertEqual(result["protocol_version"] as? String, "pep/1.7.0")
        XCTAssertEqual(result["dialect"] as? String, "swift-source")
        let capabilities = result["capabilities"] as? [String: Any]
        XCTAssertEqual(capabilities?["authoring_surfaces"] as? [String], ["swift-source"])
        XCTAssertEqual(capabilities?["emits_signed_mementos"] as? Bool, false)
    }
}

private func JSONValue(_ value: JcsCanonical) throws -> Any {
    let data = JcsCanonicalizer.encode(value)
    return try JSONSerialization.jsonObject(with: data)
}

private func JSONObject(_ value: JcsCanonical) throws -> [String: Any] {
    return try XCTUnwrap(JSONValue(value) as? [String: Any])
}

private func canonicalString(_ values: [JcsCanonical]) throws -> String {
    let data = JcsCanonicalizer.encode(.array(values))
    return try XCTUnwrap(String(data: data, encoding: .utf8))
}

private func ctorNames(_ node: Any) -> [String] {
    if let object = node as? [String: Any] {
        var names: [String] = []
        if object["kind"] as? String == "ctor", let name = object["name"] as? String {
            names.append(name)
        }
        if let args = object["args"] as? [Any] {
            for arg in args {
                names.append(contentsOf: ctorNames(arg))
            }
        }
        return names
    }
    if let array = node as? [Any] {
        return array.flatMap(ctorNames)
    }
    return []
}
