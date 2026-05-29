// SPDX-License-Identifier: Apache-2.0
//
// provekit-lift-swift-xctest-tests: lifts native XCTest assertions into
// ProofIR contracts over the lift-plugin RPC seam. Swift parsing stays in the
// Swift kit; the Rust CLI only sees normalized IR data.

import Foundation
import SwiftParser
import SwiftSyntax
import ProvekitCrypto

private let surface = "swift-xctest-tests"
private let version = "0.1.0"

private struct LiftResult {
    var ir: [JcsCanonical] = []
    var diagnostics: [JcsCanonical] = []
    var refusals: [JcsCanonical] = []
}

private struct UnsupportedAssertion: Error, CustomStringConvertible {
    let reason: String

    var description: String { reason }
}

private enum IR {
    static func primitiveSort(_ name: String) -> JcsCanonical {
        .object([
            ("kind", .string("primitive")),
            ("name", .string(name)),
        ])
    }

    static func varTerm(_ name: String) -> JcsCanonical {
        .object([
            ("kind", .string("var")),
            ("name", .string(name)),
        ])
    }

    static func intConst(_ value: Int64) -> JcsCanonical {
        const(value: .int(value), sort: "Int")
    }

    static func boolConst(_ value: Bool) -> JcsCanonical {
        const(value: .bool(value), sort: "Bool")
    }

    static func stringConst(_ value: String) -> JcsCanonical {
        const(value: .string(value), sort: "String")
    }

    static func nilConst() -> JcsCanonical {
        const(value: .null, sort: "Optional")
    }

    static func ctor(_ name: String, _ args: [JcsCanonical]) -> JcsCanonical {
        .object([
            ("kind", .string("ctor")),
            ("name", .string(name)),
            ("args", .array(args)),
        ])
    }

    static func atomic(_ name: String, _ args: [JcsCanonical]) -> JcsCanonical {
        .object([
            ("kind", .string("atomic")),
            ("name", .string(name)),
            ("args", .array(args)),
        ])
    }

    static func not(_ formula: JcsCanonical) -> JcsCanonical {
        .object([
            ("kind", .string("not")),
            ("operands", .array([formula])),
        ])
    }

    static func contract(name: String, inv: JcsCanonical, path: String, line: Int) -> JcsCanonical {
        .object([
            ("schemaVersion", .string("1")),
            ("kind", .string("contract")),
            ("name", .string(name)),
            ("outBinding", .string("out")),
            ("inv", inv),
            ("locus", .object([
                ("file", .string(path)),
                ("line", .int(Int64(line))),
                ("col", .int(1)),
            ])),
        ])
    }

    static func diagnostic(severity: String, message: String) -> JcsCanonical {
        .object([
            ("severity", .string(severity)),
            ("message", .string(message)),
        ])
    }

    static func refusal(function: String?, path: String, line: Int?, reason: String) -> JcsCanonical {
        .object([
            ("kind", .string("xctest-assertion-skip")),
            ("function", function.map(JcsCanonical.string) ?? .null),
            ("file", .string(path)),
            ("line", line.map { .int(Int64($0)) } ?? .null),
            ("reason", .string(reason)),
        ])
    }

    static func anyValue(_ value: JcsCanonical) -> Any {
        switch value {
        case .null:
            return NSNull()
        case .bool(let b):
            return b
        case .int(let n):
            return n
        case .string(let s):
            return s
        case .array(let values):
            return values.map(anyValue)
        case .object(let pairs):
            var object: [String: Any] = [:]
            for (key, value) in pairs {
                object[key] = anyValue(value)
            }
            return object
        }
    }

    private static func const(value: JcsCanonical, sort: String) -> JcsCanonical {
        .object([
            ("kind", .string("const")),
            ("sort", primitiveSort(sort)),
            ("value", value),
        ])
    }
}

private final class XCTestAssertionCollector: SyntaxVisitor {
    private let path: String
    private let converter: SourceLocationConverter
    private var functionStack: [String] = []
    private var assertionIndexByFunction: [String: Int] = [:]

    var contracts: [JcsCanonical] = []
    var refusals: [JcsCanonical] = []

    init(path: String, converter: SourceLocationConverter) {
        self.path = path
        self.converter = converter
        super.init(viewMode: .sourceAccurate)
    }

    override func visit(_ node: FunctionDeclSyntax) -> SyntaxVisitorContinueKind {
        functionStack.append(node.name.text)
        return .visitChildren
    }

    override func visitPost(_ node: FunctionDeclSyntax) {
        _ = functionStack.popLast()
    }

    override func visit(_ node: FunctionCallExprSyntax) -> SyntaxVisitorContinueKind {
        guard let assertion = assertionName(node.calledExpression) else {
            return .visitChildren
        }

        let function = functionStack.last ?? "<top-level>"
        let line = node.startLocation(converter: converter).line
        do {
            let inv = try liftAssertion(name: assertion, call: node)
            let idx = assertionIndexByFunction[function, default: 0]
            assertionIndexByFunction[function] = idx + 1
            let contractName = "\(path):\(function)::\(idx)"
            contracts.append(IR.contract(name: contractName, inv: inv, path: path, line: line))
        } catch let unsupported as UnsupportedAssertion {
            refusals.append(IR.refusal(
                function: function,
                path: path,
                line: line,
                reason: "\(assertion): \(unsupported.reason)"
            ))
        } catch {
            refusals.append(IR.refusal(
                function: function,
                path: path,
                line: line,
                reason: "\(assertion): \(error)"
            ))
        }
        return .skipChildren
    }
}

private func liftPaths(workspaceRoot: String, sourcePaths: [String]) -> LiftResult {
    var result = LiftResult()
    let root = URL(fileURLWithPath: workspaceRoot.isEmpty ? "." : workspaceRoot).standardizedFileURL
    let rootPath = root.path
    let fileManager = FileManager.default

    for requested in sourcePaths.isEmpty ? ["."] : sourcePaths {
        let requestedURL = URL(fileURLWithPath: requested, relativeTo: root).standardizedFileURL
        let fullPath = requestedURL.path
        guard isPath(fullPath, inside: rootPath) else {
            result.refusals.append(IR.refusal(
                function: nil,
                path: requested,
                line: nil,
                reason: "path escapes workspace root"
            ))
            continue
        }

        var isDirectory: ObjCBool = false
        guard fileManager.fileExists(atPath: fullPath, isDirectory: &isDirectory) else {
            result.diagnostics.append(IR.diagnostic(
                severity: "warning",
                message: "path not found: \(fullPath)"
            ))
            continue
        }

        let files: [String]
        if isDirectory.boolValue {
            let enumerator = fileManager.enumerator(atPath: fullPath)
            files = (enumerator?.compactMap { item -> String? in
                guard let rel = item as? String, rel.hasSuffix(".swift") else {
                    return nil
                }
                if rel.contains("/.build/") || rel.hasPrefix(".build/") || rel.hasPrefix(".provekit/") {
                    return nil
                }
                return URL(fileURLWithPath: rel, relativeTo: requestedURL).standardizedFileURL.path
            } ?? []).sorted()
        } else if fullPath.hasSuffix(".swift") {
            files = [fullPath]
        } else {
            files = []
        }

        for file in files {
            do {
                let source = try String(contentsOfFile: file, encoding: .utf8)
                let displayPath = relativePath(file, root: rootPath)
                let sourceFile = Parser.parse(source: source)
                let converter = SourceLocationConverter(fileName: displayPath, tree: sourceFile)
                let collector = XCTestAssertionCollector(path: displayPath, converter: converter)
                collector.walk(sourceFile)
                result.ir.append(contentsOf: collector.contracts)
                result.refusals.append(contentsOf: collector.refusals)
            } catch {
                result.refusals.append(IR.refusal(
                    function: nil,
                    path: file,
                    line: nil,
                    reason: "cannot read or parse file: \(error)"
                ))
            }
        }
    }

    return result
}

private func liftAssertion(name: String, call: FunctionCallExprSyntax) throws -> JcsCanonical {
    let args = Array(call.arguments)

    func requireArg(_ index: Int) throws -> ExprSyntax {
        guard index < args.count else {
            throw UnsupportedAssertion(reason: "missing argument \(index)")
        }
        return args[index].expression
    }

    switch name {
    case "XCTAssertEqual":
        return IR.atomic("=", [
            try liftTerm(requireArg(0)),
            try liftTerm(requireArg(1)),
        ])
    case "XCTAssertNotEqual":
        return IR.atomic("≠", [
            try liftTerm(requireArg(0)),
            try liftTerm(requireArg(1)),
        ])
    case "XCTAssertGreaterThan":
        return IR.atomic(">", [
            try liftTerm(requireArg(0)),
            try liftTerm(requireArg(1)),
        ])
    case "XCTAssertGreaterThanOrEqual":
        return IR.atomic("≥", [
            try liftTerm(requireArg(0)),
            try liftTerm(requireArg(1)),
        ])
    case "XCTAssertLessThan":
        return IR.atomic("<", [
            try liftTerm(requireArg(0)),
            try liftTerm(requireArg(1)),
        ])
    case "XCTAssertLessThanOrEqual":
        return IR.atomic("≤", [
            try liftTerm(requireArg(0)),
            try liftTerm(requireArg(1)),
        ])
    case "XCTAssertNil":
        return IR.atomic("=", [
            try liftTerm(requireArg(0)),
            IR.nilConst(),
        ])
    case "XCTAssertNotNil":
        return IR.not(IR.atomic("=", [
            try liftTerm(requireArg(0)),
            IR.nilConst(),
        ]))
    case "XCTAssertTrue":
        return try liftBooleanFormula(requireArg(0)) ?? IR.atomic("IsTrue", [try liftTerm(requireArg(0))])
    case "XCTAssertFalse":
        if let formula = try liftBooleanFormula(requireArg(0)) {
            return IR.not(formula)
        }
        return IR.atomic("IsFalse", [try liftTerm(requireArg(0))])
    default:
        throw UnsupportedAssertion(reason: "not in XCTest assertion whitelist")
    }
}

private func liftBooleanFormula(_ expr: ExprSyntax) throws -> JcsCanonical? {
    if let paren = expr.as(TupleExprSyntax.self),
       paren.elements.count == 1,
       let first = paren.elements.first {
        return try liftBooleanFormula(first.expression)
    }
    guard let sequence = expr.as(SequenceExprSyntax.self) else {
        return nil
    }
    let elements = Array(sequence.elements)
    guard elements.count == 3 else {
        return nil
    }
    let op = elements[1].description.trimmingCharacters(in: .whitespacesAndNewlines)
    let pred: String
    switch op {
    case "==": pred = "="
    case "!=": pred = "≠"
    case "<": pred = "<"
    case "<=": pred = "≤"
    case ">": pred = ">"
    case ">=": pred = "≥"
    default: return nil
    }
    return IR.atomic(pred, [
        try liftTerm(elements[0]),
        try liftTerm(elements[2]),
    ])
}

private func liftTerm(_ expr: ExprSyntax) throws -> JcsCanonical {
    if let paren = expr.as(TupleExprSyntax.self),
       paren.elements.count == 1,
       let first = paren.elements.first {
        return try liftTerm(first.expression)
    }
    if let literal = expr.as(IntegerLiteralExprSyntax.self) {
        let raw = literal.literal.text.replacingOccurrences(of: "_", with: "")
        guard let value = Int64(raw) else {
            throw UnsupportedAssertion(reason: "integer literal is out of Int64 range")
        }
        return IR.intConst(value)
    }
    if let literal = expr.as(BooleanLiteralExprSyntax.self) {
        return IR.boolConst(literal.literal.text == "true")
    }
    if expr.is(NilLiteralExprSyntax.self) {
        return IR.nilConst()
    }
    if let literal = expr.as(StringLiteralExprSyntax.self) {
        return try liftStringLiteral(literal)
    }
    if let ref = expr.as(DeclReferenceExprSyntax.self) {
        return IR.varTerm(ref.baseName.text)
    }
    if let prefix = expr.as(PrefixOperatorExprSyntax.self) {
        let op = prefix.operator.text.trimmingCharacters(in: .whitespacesAndNewlines)
        if op == "-",
           let literal = prefix.expression.as(IntegerLiteralExprSyntax.self),
           let value = Int64(literal.literal.text.replacingOccurrences(of: "_", with: "")) {
            return IR.intConst(-value)
        }
        return IR.ctor(op, [try liftTerm(prefix.expression)])
    }
    if let sequence = expr.as(SequenceExprSyntax.self) {
        let elements = Array(sequence.elements)
        guard elements.count == 3 else {
            throw UnsupportedAssertion(reason: "only single binary expressions are supported")
        }
        let op = elements[1].description.trimmingCharacters(in: .whitespacesAndNewlines)
        let ctor: String
        switch op {
        case "+": ctor = "+"
        case "-": ctor = "-"
        case "*": ctor = "*"
        case "/": ctor = "/"
        case "%": ctor = "mod"
        default:
            throw UnsupportedAssertion(reason: "unsupported term operator \(op)")
        }
        return IR.ctor(ctor, [
            try liftTerm(elements[0]),
            try liftTerm(elements[2]),
        ])
    }
    if let call = expr.as(FunctionCallExprSyntax.self) {
        guard assertionName(call.calledExpression) == nil else {
            throw UnsupportedAssertion(reason: "nested XCTest assertion is not an operand")
        }
        let callee = calleeName(call.calledExpression)
        var args: [JcsCanonical] = []
        for arg in call.arguments {
            args.append(try liftTerm(arg.expression))
        }
        return IR.ctor(callee, args)
    }
    if let member = expr.as(MemberAccessExprSyntax.self) {
        return IR.varTerm(member.description.trimmingCharacters(in: .whitespacesAndNewlines))
    }

    throw UnsupportedAssertion(reason: "unsupported operand \(type(of: expr))")
}

private func liftStringLiteral(_ node: StringLiteralExprSyntax) throws -> JcsCanonical {
    let raw = node.description.trimmingCharacters(in: .whitespacesAndNewlines)
    guard raw.hasPrefix("\""), raw.hasSuffix("\""), !raw.contains("\\(") else {
        throw UnsupportedAssertion(reason: "only non-interpolated string literals are supported")
    }
    let inner = String(raw.dropFirst().dropLast())
    let decoded = inner
        .replacingOccurrences(of: "\\n", with: "\n")
        .replacingOccurrences(of: "\\t", with: "\t")
        .replacingOccurrences(of: "\\\"", with: "\"")
        .replacingOccurrences(of: "\\\\", with: "\\")
    return IR.stringConst(decoded)
}

private func assertionName(_ expr: ExprSyntax) -> String? {
    let name = calleeName(expr)
    return name.hasPrefix("XCTAssert") ? name : nil
}

private func calleeName(_ expr: ExprSyntax) -> String {
    if let ref = expr.as(DeclReferenceExprSyntax.self) {
        return ref.baseName.text
    }
    if let member = expr.as(MemberAccessExprSyntax.self) {
        return member.declName.baseName.text
    }
    let rendered = expr.description.trimmingCharacters(in: .whitespacesAndNewlines)
    if let last = rendered.split(separator: ".").last {
        return String(last)
    }
    return rendered
}

private func isPath(_ child: String, inside root: String) -> Bool {
    if child == root {
        return true
    }
    let prefix = root.hasSuffix("/") ? root : root + "/"
    return child.hasPrefix(prefix)
}

private func relativePath(_ path: String, root: String) -> String {
    let prefix = root.hasSuffix("/") ? root : root + "/"
    if path.hasPrefix(prefix) {
        return String(path.dropFirst(prefix.count))
    }
    return path
}

private enum RPC {
    static func initializeResult() -> [String: Any] {
        [
            "name": "provekit-lift-swift-xctest-tests",
            "version": version,
            "protocol_version": "pep/1.7.0",
            "dialect": surface,
            "capabilities": [
                "authoring_surfaces": [surface],
                "ir_version": "v1.1.0",
                "emits_signed_mementos": false,
            ],
        ]
    }

    static func run() {
        while let line = readLine(strippingNewline: true) {
            let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
            if trimmed.isEmpty {
                continue
            }

            let response: [String: Any]
            do {
                guard let data = trimmed.data(using: .utf8),
                      let request = try JSONSerialization.jsonObject(with: data) as? [String: Any]
                else {
                    write(errorResponse(id: nil, code: -32700, message: "PARSE_ERROR"))
                    continue
                }
                response = try dispatch(request)
            } catch let exit as RPCExit {
                write(exit.response)
                return
            } catch {
                response = errorResponse(id: nil, code: -32603, message: String(describing: error))
            }
            write(response)
        }
    }

    private static func dispatch(_ request: [String: Any]) throws -> [String: Any] {
        let id = request["id"] ?? NSNull()
        let method = request["method"] as? String ?? ""
        let params = request["params"] as? [String: Any] ?? [:]

        switch method {
        case "initialize":
            return response(id: id, result: initializeResult())
        case "lift":
            return lift(id: id, params: params)
        case "shutdown", "exit":
            throw RPCExit(response: response(id: id, result: NSNull()))
        default:
            return errorResponse(id: id, code: -32601, message: "METHOD_NOT_FOUND: \(method)")
        }
    }

    private static func lift(id: Any, params: [String: Any]) -> [String: Any] {
        let requestedSurface = params["surface"] as? String ?? surface
        guard requestedSurface == surface else {
            return errorResponse(id: id, code: 1003, message: "SURFACE_NOT_SUPPORTED: \(requestedSurface)")
        }

        let sourcePaths = (params["source_paths"] as? [Any])?
            .compactMap { $0 as? String }
            .filter { !$0.isEmpty } ?? ["."]
        let workspaceRoot = params["workspace_root"] as? String ?? "."
        let result = liftPaths(workspaceRoot: workspaceRoot, sourcePaths: sourcePaths)

        return response(id: id, result: [
            "kind": "ir-document",
            "ir": result.ir.map(IR.anyValue),
            "callEdges": [],
            "diagnostics": result.diagnostics.map(IR.anyValue),
            "refusals": result.refusals.map(IR.anyValue),
        ])
    }

    private static func response(id: Any, result: Any) -> [String: Any] {
        [
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        ]
    }

    private static func errorResponse(id: Any?, code: Int, message: String) -> [String: Any] {
        [
            "jsonrpc": "2.0",
            "id": id ?? NSNull(),
            "error": [
                "code": code,
                "message": message,
            ],
        ]
    }

    private static func write(_ object: [String: Any]) {
        guard JSONSerialization.isValidJSONObject(object),
              let data = try? JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
        else {
            FileHandle.standardOutput.write(#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"SERIALIZE_ERROR"}}"#.data(using: .utf8)!)
            FileHandle.standardOutput.write(Data([0x0A]))
            return
        }
        FileHandle.standardOutput.write(data)
        FileHandle.standardOutput.write(Data([0x0A]))
        fflush(stdout)
    }
}

private struct RPCExit: Error, @unchecked Sendable {
    let response: [String: Any]
}

if CommandLine.arguments.contains("--rpc") {
    RPC.run()
} else {
    FileHandle.standardError.write("usage: provekit-lift-swift-xctest-tests --rpc\n".data(using: .utf8)!)
    exit(1)
}
