// SPDX-License-Identifier: Apache-2.0

import Foundation
import SwiftParser
import ProvekitCrypto

public enum SwiftSourceCompiler {
    public static func compileIRDocument(_ ir: [JcsCanonical]) throws -> String {
        var functions: [String] = []
        for contract in ir where stringField(contract, "kind") == "function-contract" {
            let body = try SwiftSourceIR.bodyTerm(of: contract)
            if ctorName(body) == "swift:source-unit" {
                continue
            }
            functions.append(try compileContract(contract))
        }
        let source = functions.joined(separator: "\n\n")
        return Parser.parse(source: source + (source.isEmpty ? "" : "\n")).description
    }

    public static func compileBodyTerm(
        _ term: JcsCanonical,
        fnName: String = "f",
        formals: [String] = []
    ) throws -> String {
        var context = CompileContext(formals: Set(formals))
        let body = try context.statementBlock(term, indent: 1)
        let params = formals.map { "_ \($0): Int" }.joined(separator: ", ")
        let raw = """
        func \(fnName)(\(params)) -> Int {
        \(body)
        }

        """
        return Parser.parse(source: raw).description
    }

    private static func compileContract(_ contract: JcsCanonical) throws -> String {
        let fnName = sourceFunctionName(stringField(contract, "fnName") ?? "f")
        let formals = arrayField(contract, "formals").compactMap(stringValue)
        let returnType = sortName(objectField(contract, "returnSort")) ?? "Int"
        var context = CompileContext(formals: Set(formals))
        let body = try context.statementBlock(SwiftSourceIR.bodyTerm(of: contract), indent: 1)
        let params = formals.map { "_ \($0): Int" }.joined(separator: ", ")
        return """
        func \(fnName)(\(params)) -> \(returnType) {
        \(body)
        }
        """
    }
}

private struct CompileContext {
    var formals: Set<String>
    var declared: Set<String>

    init(formals: Set<String>) {
        self.formals = formals
        self.declared = formals
    }

    mutating func statementBlock(_ term: JcsCanonical, indent: Int) throws -> String {
        let lines = try statements(term, indent: indent)
        if lines.isEmpty {
            return "\(spaces(indent))_ = 0"
        }
        return lines.joined(separator: "\n")
    }

    mutating func statements(_ term: JcsCanonical, indent: Int) throws -> [String] {
        if ctorName(term) == "swift:seq" {
            let args = ctorArgs(term)
            guard args.count == 2 else {
                throw SwiftSourceError.invalidIR("swift:seq expects two arguments")
            }
            return try statements(args[0], indent: indent) + statements(args[1], indent: indent)
        }
        return [try statement(term, indent: indent)]
    }

    mutating func statement(_ term: JcsCanonical, indent: Int) throws -> String {
        let prefix = spaces(indent)
        guard let name = ctorName(term) else {
            return "\(prefix)_ = \(try expression(term))"
        }
        let args = ctorArgs(term)
        switch name {
        case "swift:empty":
            return "\(prefix)_ = 0"
        case "swift:assign":
            guard args.count == 2 else { throw SwiftSourceError.invalidIR("swift:assign expects two arguments") }
            let target = try lvalue(args[0])
            let value = try expression(args[1])
            if case .object(let pairs) = args[0],
               pairs.contains(where: { $0.0 == "kind" && $0.1 == .string("var") }),
               let varName = stringField(args[0], "name"),
               !declared.contains(varName) {
                declared.insert(varName)
                return "\(prefix)var \(varName) = \(value)"
            }
            return "\(prefix)\(target) = \(value)"
        case "swift:return":
            guard args.count == 1 else { throw SwiftSourceError.invalidIR("swift:return expects one argument") }
            return "\(prefix)return \(try expression(args[0]))"
        case "swift:if":
            guard args.count == 3 else { throw SwiftSourceError.invalidIR("swift:if expects three arguments") }
            let cond = try expression(args[0])
            let thenBody = try nestedBlock(args[1], indent: indent + 1)
            let elseBody = try nestedBlock(args[2], indent: indent + 1)
            return """
            \(prefix)if \(cond) {
            \(thenBody)
            \(prefix)} else {
            \(elseBody)
            \(prefix)}
            """
        case "swift:while":
            guard args.count == 2 else { throw SwiftSourceError.invalidIR("swift:while expects two arguments") }
            let body = try nestedBlock(args[1], indent: indent + 1)
            return """
            \(prefix)while \(try expression(args[0])) {
            \(body)
            \(prefix)}
            """
        case "swift:repeat":
            guard args.count == 2 else { throw SwiftSourceError.invalidIR("swift:repeat expects two arguments") }
            let body = try nestedBlock(args[0], indent: indent + 1)
            return """
            \(prefix)repeat {
            \(body)
            \(prefix)} while \(try expression(args[1]))
            """
        case "swift:for":
            guard args.count == 3 else { throw SwiftSourceError.invalidIR("swift:for expects three arguments") }
            let loopVar = try lvalue(args[0])
            let old = declared
            declared.insert(loopVar)
            let body = try nestedBlock(args[2], indent: indent + 1)
            declared = old
            return """
            \(prefix)for \(loopVar) in \(try expression(args[1])) {
            \(body)
            \(prefix)}
            """
        case "swift:break":
            return "\(prefix)break"
        case "swift:continue":
            return "\(prefix)continue"
        case "swift:throw":
            guard args.count == 1 else { throw SwiftSourceError.invalidIR("swift:throw expects one argument") }
            return "\(prefix)throw \(try expression(args[0]))"
        case "swift:call":
            return "\(prefix)\(try expression(term))"
        default:
            return "\(prefix)_ = \(try expression(term))"
        }
    }

    private mutating func nestedBlock(_ term: JcsCanonical, indent: Int) throws -> String {
        let nested = try statements(term, indent: indent)
        return nested.isEmpty ? "\(spaces(indent))_ = 0" : nested.joined(separator: "\n")
    }

    mutating func expression(_ term: JcsCanonical) throws -> String {
        if kind(term) == "const" {
            return try constExpression(term)
        }
        if kind(term) == "var" {
            return stringField(term, "name") ?? "x"
        }
        guard let name = ctorName(term) else {
            throw SwiftSourceError.invalidIR("unsupported term in expression position")
        }
        let args = ctorArgs(term)
        switch name {
        case "swift:add": return try binary(args, "+")
        case "swift:sub": return try binary(args, "-")
        case "swift:mul": return try binary(args, "*")
        case "swift:div": return try binary(args, "/")
        case "swift:mod": return try binary(args, "%")
        case "swift:eq": return try binary(args, "==")
        case "swift:ne": return try binary(args, "!=")
        case "swift:lt": return try binary(args, "<")
        case "swift:le": return try binary(args, "<=")
        case "swift:gt": return try binary(args, ">")
        case "swift:ge": return try binary(args, ">=")
        case "swift:and": return try binary(args, "&&")
        case "swift:or": return try binary(args, "||")
        case "swift:nilcoalesce": return try binary(args, "??")
        case "swift:bitand": return try binary(args, "&")
        case "swift:bitor": return try binary(args, "|")
        case "swift:bitxor": return try binary(args, "^")
        case "swift:shl": return try binary(args, "<<")
        case "swift:shr": return try binary(args, ">>")
        case "swift:not":
            return "(!\(try expression(args[0])))"
        case "swift:neg":
            return "(-\(try expression(args[0])))"
        case "swift:pos":
            return "(+\(try expression(args[0])))"
        case "swift:ternary":
            guard args.count == 3 else { throw SwiftSourceError.invalidIR("swift:ternary expects three arguments") }
            return "(\(try expression(args[0])) ? \(try expression(args[1])) : \(try expression(args[2])))"
        case "swift:member":
            guard args.count == 2 else { throw SwiftSourceError.invalidIR("swift:member expects two arguments") }
            return "\(try expression(args[0])).\(stringConst(args[1]))"
        case "swift:index":
            guard args.count == 2 else { throw SwiftSourceError.invalidIR("swift:index expects two arguments") }
            return "\(try expression(args[0]))[\(try expression(args[1]))]"
        case "swift:call":
            guard !args.isEmpty else { throw SwiftSourceError.invalidIR("swift:call expects callee") }
            let callee = stringConst(args[0])
            let callArgs = try args.dropFirst().map { try expression($0) }.joined(separator: ", ")
            return "\(callee)(\(callArgs))"
        case "swift:assign":
            guard args.count == 2 else { throw SwiftSourceError.invalidIR("swift:assign expects two arguments") }
            return "(\(try lvalue(args[0])) = \(try expression(args[1])))"
        default:
            throw SwiftSourceError.unsupportedOperation("unsupported Swift operation in expression position: \(name)")
        }
    }

    mutating func lvalue(_ term: JcsCanonical) throws -> String {
        if kind(term) == "var" {
            return stringField(term, "name") ?? "x"
        }
        return try expression(term)
    }

    private mutating func binary(_ args: [JcsCanonical], _ op: String) throws -> String {
        guard args.count == 2 else {
            throw SwiftSourceError.invalidIR("binary operation \(op) expects two arguments")
        }
        return "(\(try expression(args[0])) \(op) \(try expression(args[1])))"
    }

    private func constExpression(_ term: JcsCanonical) throws -> String {
        guard let value = objectField(term, "value") else {
            throw SwiftSourceError.invalidIR("const missing value")
        }
        switch value {
        case .null:
            return "nil"
        case .bool(let b):
            return b ? "true" : "false"
        case .int(let n):
            return String(n)
        case .string(let s):
            return "\"\(escapeSwiftString(s))\""
        default:
            throw SwiftSourceError.invalidIR("unsupported const value")
        }
    }
}

private func sourceFunctionName(_ fnName: String) -> String {
    let beforeSignature = fnName.split(separator: "(", maxSplits: 1).first.map(String.init) ?? fnName
    return beforeSignature.split(separator: ".").last.map(String.init) ?? "f"
}

private func spaces(_ indent: Int) -> String {
    String(repeating: "    ", count: indent)
}

private func escapeSwiftString(_ s: String) -> String {
    s.replacingOccurrences(of: "\\", with: "\\\\")
        .replacingOccurrences(of: "\"", with: "\\\"")
        .replacingOccurrences(of: "\n", with: "\\n")
        .replacingOccurrences(of: "\t", with: "\\t")
}

private func kind(_ value: JcsCanonical) -> String? {
    stringField(value, "kind")
}

private func ctorName(_ value: JcsCanonical) -> String? {
    guard kind(value) == "ctor" else {
        return nil
    }
    return stringField(value, "name")
}

private func ctorArgs(_ value: JcsCanonical) -> [JcsCanonical] {
    arrayField(value, "args")
}

private func stringConst(_ value: JcsCanonical) -> String {
    guard kind(value) == "const", case .string(let s)? = objectField(value, "value") else {
        return ""
    }
    return s
}

private func stringValue(_ value: JcsCanonical) -> String? {
    guard case .string(let s) = value else {
        return nil
    }
    return s
}

private func stringField(_ value: JcsCanonical, _ key: String) -> String? {
    guard case .string(let s)? = objectField(value, key) else {
        return nil
    }
    return s
}

private func objectField(_ value: JcsCanonical, _ key: String) -> JcsCanonical? {
    guard case .object(let pairs) = value else {
        return nil
    }
    return pairs.first(where: { $0.0 == key })?.1
}

private func arrayField(_ value: JcsCanonical, _ key: String) -> [JcsCanonical] {
    guard case .array(let values)? = objectField(value, key) else {
        return []
    }
    return values
}

private func sortName(_ value: JcsCanonical?) -> String? {
    guard let value else { return nil }
    return stringField(value, "name")
}
