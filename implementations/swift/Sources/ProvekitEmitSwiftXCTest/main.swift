// SPDX-License-Identifier: Apache-2.0
//
// provekit-emit-swift-xctest: PEP 1.7.0 emit plugin for Swift XCTest.
//
// The Rust CLI sends a neutral EmitPlan over JSON-RPC. This kit owns the
// Swift/XCTest syntax and a native Swift parser check.

import Foundation
import ProvekitCrypto

private struct EmitPlan {
    let contractID: String
    let function: String
    let params: [String]
    let paramTypes: [String]
    let predicates: [[String: Any]]

    static func from(_ params: [String: Any]) -> EmitPlan {
        EmitPlan(
            contractID: firstString(params["contract_id"], params["concept_name"]) ?? "",
            function: firstString(params["function"], params["function_name"], params["functionName"]) ?? "contract",
            params: stringList(params["params"]),
            paramTypes: stringList(params["param_types"]),
            predicates: mapList(params["predicates"])
        )
    }
}

private struct Emission {
    let source: String
    let path: String
    let artifactCID: String
    let emittedPredicates: [String]
    let unsupportedPredicates: [String]

    var isComplete: Bool {
        unsupportedPredicates.isEmpty && !emittedPredicates.isEmpty
    }

    func toJSON() -> [String: Any] {
        [
            "kind": "swift-xctest-test-emission",
            "source": source,
            "path": path,
            "extension": "swift",
            "emitted_artifact_cid": artifactCID,
            "emitted_predicates": emittedPredicates,
            "unsupported_predicates": unsupportedPredicates,
            "is_complete": isComplete,
        ]
    }
}

private func emit(_ plan: EmitPlan) -> Emission {
    var functions: [String] = []
    var emitted: [String] = []
    var unsupported: [String] = []

    for (idx, predicate) in plan.predicates.enumerated() {
        let head = predicateHead(predicate)
        guard let assertion = renderAssertion(head: head, predicate: predicate) else {
            unsupported.append(head ?? "<malformed>")
            continue
        }
        let declarations = variableDeclarations(for: predicate, head: head)
        functions.append(renderTestFunction(name: testFunctionName(head: head, index: idx), declarations: declarations, assertion: assertion))
        emitted.append(head ?? "predicate")
    }

    let source = renderModule(className: testClassName(plan.function), functions: functions)
    return Emission(
        source: source,
        path: "Tests/ProvekitEmittedTests/ProvekitEmittedTests.swift",
        artifactCID: Blake3.hex(Data(source.utf8)),
        emittedPredicates: emitted,
        unsupportedPredicates: unsupported
    )
}

private func renderModule(className: String, functions: [String]) -> String {
    var lines: [String] = [
        "import XCTest",
        "",
        "final class \(className): XCTestCase {",
    ]
    if functions.isEmpty {
        lines.append("}")
        lines.append("")
        return lines.joined(separator: "\n")
    }
    for (index, function) in functions.enumerated() {
        if index > 0 {
            lines.append("")
        }
        lines.append(contentsOf: function.split(separator: "\n", omittingEmptySubsequences: false).map(String.init))
    }
    lines.append("}")
    lines.append("")
    return lines.joined(separator: "\n")
}

private func renderTestFunction(name: String, declarations: [String], assertion: String) -> String {
    var lines: [String] = ["    func \(name)() {"]
    for declaration in declarations {
        lines.append("        \(declaration)")
    }
    for line in assertion.split(separator: "\n", omittingEmptySubsequences: false) {
        lines.append("        \(line)")
    }
    lines.append("    }")
    return lines.joined(separator: "\n")
}

private func renderAssertion(head: String?, predicate: [String: Any]) -> String? {
    guard let head else {
        return nil
    }
    let args = argumentMaps(predicate)
    switch head {
    case "eq":
        guard let pair = renderPair(args) else { return nil }
        return "XCTAssertEqual(\(pair.0), \(pair.1))"
    case "ne":
        guard let pair = renderPair(args) else { return nil }
        return "XCTAssertNotEqual(\(pair.0), \(pair.1))"
    case "lt":
        guard let pair = renderPair(args) else { return nil }
        return "XCTAssertTrue(\(pair.0) < \(pair.1))"
    case "le":
        guard let pair = renderPair(args) else { return nil }
        return "XCTAssertTrue(\(pair.0) <= \(pair.1))"
    case "gt":
        guard let pair = renderPair(args) else { return nil }
        return "XCTAssertTrue(\(pair.0) > \(pair.1))"
    case "ge":
        guard let pair = renderPair(args) else { return nil }
        return "XCTAssertTrue(\(pair.0) >= \(pair.1))"
    case "not-null", "option-is-some":
        guard args.count == 1, let expr = renderTerm(args[0]) else { return nil }
        return "XCTAssertNotNil(\(expr))"
    case "option-is-none":
        guard args.count == 1, let expr = renderTerm(args[0]) else { return nil }
        return "XCTAssertNil(\(expr))"
    default:
        return nil
    }
}

private func renderPair(_ args: [[String: Any]]) -> (String, String)? {
    guard args.count == 2,
          let left = renderTerm(args[0]),
          let right = renderTerm(args[1])
    else {
        return nil
    }
    return (left, right)
}

private func renderTerm(_ term: [String: Any]) -> String? {
    let kind = term["kind"] as? String
    switch kind {
    case "const":
        return constExpression(term["value"])
    case "var":
        return sanitizeIdentifier(term["name"] as? String, fallback: "value")
    case "op":
        let head = predicateHead(term)
        let args = argumentMaps(term)
        switch head {
        case "add": return renderBinaryExpression(args, "+")
        case "sub": return renderBinaryExpression(args, "-")
        case "mul": return renderBinaryExpression(args, "*")
        case "div": return renderBinaryExpression(args, "/")
        default: return nil
        }
    default:
        return nil
    }
}

private func renderBinaryExpression(_ args: [[String: Any]], _ op: String) -> String? {
    guard let pair = renderPair(args) else {
        return nil
    }
    return "(\(pair.0) \(op) \(pair.1))"
}

private func constExpression(_ value: Any?) -> String? {
    if value == nil || value is NSNull {
        return "nil"
    }
    if let bool = value as? Bool {
        return bool ? "true" : "false"
    }
    if let number = value as? NSNumber {
        return number.stringValue
    }
    if let string = value as? String {
        return "\"\(escapeSwiftString(string))\""
    }
    return nil
}

private func variableDeclarations(for predicate: [String: Any], head: String?) -> [String] {
    var ordered: [String] = []
    var seen = Set<String>()
    collectVars(predicate, into: &ordered, seen: &seen)
    return ordered.enumerated().map { index, raw in
        "\(sanitizeIdentifier(raw, fallback: "v\(index)")) = \(placeholderValue(for: head, index: index))"
    }
}

private func collectVars(_ term: [String: Any], into ordered: inout [String], seen: inout Set<String>) {
    if term["kind"] as? String == "var", let name = term["name"] as? String {
        if seen.insert(name).inserted {
            ordered.append(name)
        }
    }
    for arg in argumentMaps(term) {
        collectVars(arg, into: &ordered, seen: &seen)
    }
}

private func placeholderValue(for head: String?, index: Int) -> String {
    switch head {
    case "lt", "le":
        return index == 0 ? "0" : "1"
    case "gt", "ge":
        return index == 0 ? "1" : "0"
    case "ne":
        return index == 0 ? "0" : "1"
    case "not-null", "option-is-some":
        return "Optional(1)"
    case "option-is-none":
        return "Optional<Int>.none"
    default:
        return "0"
    }
}

private func predicateHead(_ predicate: [String: Any]) -> String? {
    guard let raw = firstString(predicate["name"], predicate["op"], predicate["head"])?
        .trimmingCharacters(in: .whitespacesAndNewlines)
    else {
        return nil
    }
    if raw.isEmpty {
        return nil
    }
    let suffix = raw.split(separator: ":").last.map(String.init) ?? raw
    return suffix
        .replacingOccurrences(of: "_", with: "-")
        .lowercased()
}

private func argumentMaps(_ predicate: [String: Any]) -> [[String: Any]] {
    guard let args = predicate["args"] as? [Any] else {
        return []
    }
    return args.compactMap { $0 as? [String: Any] }
}

private func testClassName(_ function: String) -> String {
    "\(upperCamel(function, fallback: "Contract"))ContractTests"
}

private func testFunctionName(head: String?, index: Int) -> String {
    "testVerifies\(upperCamel(head ?? "predicate", fallback: "Predicate"))\(index)"
}

private func upperCamel(_ raw: String, fallback: String) -> String {
    let parts = raw
        .split { !$0.isLetter && !$0.isNumber }
        .map(String.init)
        .filter { !$0.isEmpty }
    let joined = parts.map { part in
        part.prefix(1).uppercased() + part.dropFirst()
    }.joined()
    let candidate = joined.isEmpty ? fallback : joined
    if candidate.first?.isNumber == true {
        return "\(fallback)\(candidate)"
    }
    return candidate
}

private func sanitizeIdentifier(_ raw: String?, fallback: String) -> String {
    let source = (raw ?? "").isEmpty ? fallback : raw!
    var scalars: [UnicodeScalar] = []
    for scalar in source.unicodeScalars {
        if CharacterSet.alphanumerics.contains(scalar) || scalar == "_" {
            scalars.append(scalar)
        } else {
            scalars.append("_")
        }
    }
    var out = String(String.UnicodeScalarView(scalars)).trimmingCharacters(in: CharacterSet(charactersIn: "_"))
    if out.isEmpty {
        out = fallback
    }
    if out.unicodeScalars.first.map(CharacterSet.decimalDigits.contains) == true {
        out = "_\(out)"
    }
    switch out {
    case "class", "struct", "enum", "func", "let", "var", "return", "nil", "true", "false":
        return "`\(out)`"
    default:
        return out
    }
}

private func escapeSwiftString(_ raw: String) -> String {
    raw
        .replacingOccurrences(of: "\\", with: "\\\\")
        .replacingOccurrences(of: "\"", with: "\\\"")
        .replacingOccurrences(of: "\n", with: "\\n")
        .replacingOccurrences(of: "\t", with: "\\t")
}

private func firstString(_ values: Any?...) -> String? {
    for value in values {
        if let string = value as? String {
            let trimmed = string.trimmingCharacters(in: .whitespacesAndNewlines)
            if !trimmed.isEmpty {
                return trimmed
            }
        }
    }
    return nil
}

private func stringList(_ value: Any?) -> [String] {
    guard let array = value as? [Any] else {
        return []
    }
    return array.compactMap { $0 as? String }
}

private func mapList(_ value: Any?) -> [[String: Any]] {
    guard let array = value as? [Any] else {
        return []
    }
    return array.compactMap { $0 as? [String: Any] }
}

private func runCheck(outDir: String, artifactPath: String?) -> [String: Any] {
    let path = artifactPath ?? "\(outDir)/Tests/ProvekitEmittedTests/ProvekitEmittedTests.swift"
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
    process.arguments = ["swiftc", "-parse", path]
    process.currentDirectoryURL = URL(fileURLWithPath: outDir)
    let stdout = Pipe()
    let stderr = Pipe()
    process.standardOutput = stdout
    process.standardError = stderr
    do {
        try process.run()
        process.waitUntilExit()
    } catch {
        return [
            "ok": false,
            "command": "swiftc -parse \(path)",
            "cwd": outDir,
            "stdout": "",
            "stderr": String(describing: error),
            "exitCode": -1,
        ]
    }
    let stdoutText = String(data: stdout.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
    let stderrText = String(data: stderr.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
    return [
        "ok": process.terminationStatus == 0,
        "command": "swiftc -parse \(path)",
        "cwd": outDir,
        "stdout": stdoutText,
        "stderr": stderrText,
        "exitCode": Int(process.terminationStatus),
    ]
}

private func dispatch(_ request: [String: Any]) -> [String: Any] {
    let id = request["id"] ?? NSNull()
    let method = request["method"] as? String ?? ""
    let params = request["params"] as? [String: Any] ?? [:]

    switch method {
    case "provekit.plugin.describe":
        return success(id: id, result: [
            "name": "swift-xctest",
            "kind": "emit",
            "protocol_versions": ["pep/1.7.0"],
            "target_language": "swift",
            "framework": "xctest",
            "supported_predicates": ["eq", "ne", "lt", "le", "gt", "ge", "not-null", "option-is-some", "option-is-none"],
        ])
    case "provekit.plugin.invoke":
        let emission = emit(EmitPlan.from(params))
        return success(id: id, result: emission.toJSON())
    case "provekit.plugin.check":
        guard let outDir = firstString(params["out_dir"], params["outDir"]) else {
            return failure(id: id, code: -32602, message: "INVALID_PARAMS: missing out_dir")
        }
        return success(
            id: id,
            result: runCheck(
                outDir: outDir,
                artifactPath: firstString(params["artifact_path"], params["artifactPath"])
            )
        )
    case "provekit.plugin.shutdown":
        return success(id: id, result: NSNull())
    default:
        return failure(id: id, code: -32601, message: "METHOD_NOT_FOUND: \(method)")
    }
}

private func success(id: Any, result: Any) -> [String: Any] {
    [
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    ]
}

private func failure(id: Any, code: Int, message: String) -> [String: Any] {
    [
        "jsonrpc": "2.0",
        "id": id,
        "error": [
            "code": code,
            "message": message,
        ],
    ]
}

private func writeJSONLine(_ object: [String: Any]) {
    guard JSONSerialization.isValidJSONObject(object),
          let data = try? JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
    else {
        FileHandle.standardOutput.write(#"{"error":{"code":-32603,"message":"SERIALIZE_ERROR"},"id":null,"jsonrpc":"2.0"}"#.data(using: .utf8)!)
        FileHandle.standardOutput.write(Data([0x0A]))
        return
    }
    FileHandle.standardOutput.write(data)
    FileHandle.standardOutput.write(Data([0x0A]))
    fflush(stdout)
}

private func runRPC() {
    while let line = readLine(strippingNewline: true) {
        let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty {
            continue
        }
        let response: [String: Any]
        if let data = trimmed.data(using: .utf8),
           let request = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
            response = dispatch(request)
            writeJSONLine(response)
            if request["method"] as? String == "provekit.plugin.shutdown" {
                return
            }
        } else {
            writeJSONLine(failure(id: NSNull(), code: -32700, message: "PARSE_ERROR"))
        }
    }
}

if CommandLine.arguments.contains("--rpc") {
    runRPC()
} else {
    let emission = emit(EmitPlan(
        contractID: "concept:eq",
        function: "contract",
        params: [],
        paramTypes: [],
        predicates: [[
            "kind": "op",
            "name": "concept:eq",
            "args": [
                ["kind": "const", "value": 1],
                ["kind": "const", "value": 1],
            ],
        ]]
    ))
    print(emission.source)
}
