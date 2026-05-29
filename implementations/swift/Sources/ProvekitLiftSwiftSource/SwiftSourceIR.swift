// SPDX-License-Identifier: Apache-2.0

import Foundation
import ProvekitCrypto

public struct SwiftSourceLiftResult: Sendable {
    public var ir: [JcsCanonical]
    public var diagnostics: [JcsCanonical]
    public var opacityReport: [JcsCanonical]
    public var refusals: [JcsCanonical]

    public init(
        ir: [JcsCanonical] = [],
        diagnostics: [JcsCanonical] = [],
        opacityReport: [JcsCanonical] = [],
        refusals: [JcsCanonical] = []
    ) {
        self.ir = ir
        self.diagnostics = diagnostics
        self.opacityReport = opacityReport
        self.refusals = refusals
    }
}

public enum SwiftSourceError: Error, CustomStringConvertible {
    case invalidIR(String)
    case unsupportedOperation(String)

    public var description: String {
        switch self {
        case .invalidIR(let message): return message
        case .unsupportedOperation(let message): return message
        }
    }
}

public enum SwiftSourceIR {
    public static func primitiveSort(_ name: String) -> JcsCanonical {
        .object([
            ("kind", .string("primitive")),
            ("name", .string(name)),
        ])
    }

    public static func trueFormula() -> JcsCanonical {
        .object([
            ("kind", .string("atomic")),
            ("name", .string("true")),
            ("args", .array([])),
        ])
    }

    public static func eqFormula(_ lhs: JcsCanonical, _ rhs: JcsCanonical) -> JcsCanonical {
        .object([
            ("kind", .string("atomic")),
            ("name", .string("=")),
            ("args", .array([lhs, rhs])),
        ])
    }

    public static func varTerm(_ name: String) -> JcsCanonical {
        .object([
            ("kind", .string("var")),
            ("name", .string(name)),
        ])
    }

    public static func intConst(_ value: Int64) -> JcsCanonical {
        const(value: .int(value), sort: "Int")
    }

    public static func boolConst(_ value: Bool) -> JcsCanonical {
        .object([
            ("kind", .string("const")),
            ("sort", primitiveSort("Bool")),
            ("value", .bool(value)),
        ])
    }

    public static func stringConst(_ value: String) -> JcsCanonical {
        const(value: .string(value), sort: "String")
    }

    public static func nilConst() -> JcsCanonical {
        const(value: .null, sort: "Optional")
    }

    public static func unitConst() -> JcsCanonical {
        const(value: .null, sort: "Unit")
    }

    public static func ctor(_ name: String, _ args: [JcsCanonical]) -> JcsCanonical {
        precondition(name.hasPrefix("swift:"), "Swift source operations must use swift: namespace")
        let local = String(name.dropFirst("swift:".count))
        precondition(local != "unknown" && local != "binop" && local != "skip",
                     "forbidden Swift source operation: \(name)")
        return .object([
            ("kind", .string("ctor")),
            ("name", .string(name)),
            ("args", .array(args)),
        ])
    }

    public static func empty() -> JcsCanonical {
        ctor("swift:empty", [])
    }

    public static func seq(_ first: JcsCanonical, _ second: JcsCanonical) -> JcsCanonical {
        ctor("swift:seq", [first, second])
    }

    public static func foldSeq(_ statements: [JcsCanonical]) -> JcsCanonical {
        guard var result = statements.first else {
            return empty()
        }
        for statement in statements.dropFirst() {
            result = seq(result, statement)
        }
        return result
    }

    public static func locus(path: String, line: Int, col: Int = 1) -> JcsCanonical {
        .object([
            ("file", .string(path)),
            ("line", .int(Int64(line))),
            ("col", .int(Int64(col))),
        ])
    }

    public static func functionContract(
        fnName: String,
        formals: [String],
        formalSorts: [String],
        returnSort: String,
        bodyTerm: JcsCanonical,
        effects: [JcsCanonical],
        sourcePath: String,
        line: Int
    ) -> JcsCanonical {
        .object([
            ("schemaVersion", .string("1")),
            ("kind", .string("function-contract")),
            ("fnName", .string(fnName)),
            ("formals", .array(formals.map { .string($0) })),
            ("formalSorts", .array(formalSorts.map(primitiveSort))),
            ("returnSort", primitiveSort(returnSort)),
            ("pre", trueFormula()),
            ("post", eqFormula(varTerm("return_value"), bodyTerm)),
            ("bodyCid", .null),
            ("effects", .array(effects)),
            ("locus", locus(path: sourcePath, line: line)),
            ("autoMintedMementos", .array([])),
        ])
    }

    public static func sourceUnitContract(
        sourcePath: String,
        source: String,
        operationalTerm: JcsCanonical
    ) -> JcsCanonical {
        .object([
            ("schemaVersion", .string("1")),
            ("kind", .string("function-contract")),
            ("fnName", .string("<source-unit:\(sourcePath)>")),
            ("formals", .array([])),
            ("formalSorts", .array([])),
            ("returnSort", primitiveSort("Stmt")),
            ("pre", trueFormula()),
            ("post", eqFormula(
                varTerm("return_value"),
                ctor("swift:source-unit", [stringConst(source), operationalTerm])
            )),
            ("bodyCid", .null),
            ("effects", .array([])),
            ("locus", locus(path: sourcePath, line: 1)),
            ("autoMintedMementos", .array([])),
        ])
    }

    public static func refusal(kind: String, function: String?, line: Int?, reason: String) -> JcsCanonical {
        .object([
            ("kind", .string(kind)),
            ("function", function.map(JcsCanonical.string) ?? .null),
            ("line", line.map { .int(Int64($0)) } ?? .null),
            ("reason", .string(reason)),
        ])
    }

    public static func fnName(of contract: JcsCanonical) -> String {
        guard case .object(let pairs) = contract,
              let value = pairs.first(where: { $0.0 == "fnName" })?.1,
              case .string(let name) = value
        else {
            return ""
        }
        return name
    }

    public static func bodyTerm(of contract: JcsCanonical) throws -> JcsCanonical {
        guard case .object(let pairs) = contract,
              let post = pairs.first(where: { $0.0 == "post" })?.1,
              case .object(let postPairs) = post,
              let args = postPairs.first(where: { $0.0 == "args" })?.1,
              case .array(let postArgs) = args,
              postArgs.count >= 2
        else {
            throw SwiftSourceError.invalidIR("function-contract post term is missing")
        }
        return postArgs[1]
    }

    public static func verifyFacingResult(_ result: SwiftSourceLiftResult) -> SwiftSourceLiftResult {
        var out = result
        out.ir = result.ir.compactMap(verifyFacingContract)
        return out
    }

    private static func verifyFacingContract(_ contract: JcsCanonical) -> JcsCanonical? {
        guard field("kind", in: contract).stringValue == "function-contract" else {
            return contract
        }
        let fnName = field("fnName", in: contract).stringValue ?? ""
        if fnName.hasPrefix("<source-unit:") {
            return nil
        }

        guard case .object(let pairs) = contract else {
            return contract
        }
        var rewritten: [(String, JcsCanonical)] = []
        var sawBridgeSourceSymbol = false
        for (key, value) in pairs {
            switch key {
            case "pre":
                rewritten.append((key, normalizeFormulaForVerify(value)))
            case "post":
                rewritten.append((key, normalizeFormulaForVerify(value)))
            case "bridgeSourceSymbol":
                sawBridgeSourceSymbol = true
                rewritten.append((key, .string(simpleFunctionSymbol(fnName))))
            default:
                rewritten.append((key, value))
            }
        }
        if !sawBridgeSourceSymbol {
            rewritten.append(("bridgeSourceSymbol", .string(simpleFunctionSymbol(fnName))))
        }
        return .object(rewritten)
    }

    private static func normalizeFormulaForVerify(_ formula: JcsCanonical) -> JcsCanonical {
        guard case .object(let pairs) = formula,
              let kind = pairs.first(where: { $0.0 == "kind" })?.1.stringValue
        else {
            return formula
        }

        switch kind {
        case "atomic":
            return .object(pairs.map { key, value in
                if key == "name", let name = value.stringValue {
                    return (key, .string(normalizeAtomicName(name)))
                }
                if key == "args", case .array(let args) = value {
                    return (key, .array(args.map(normalizeTermForVerify)))
                }
                return (key, value)
            })
        case "and", "or", "not", "implies":
            return .object(pairs.map { key, value in
                if key == "operands", case .array(let operands) = value {
                    return (key, .array(operands.map(normalizeFormulaForVerify)))
                }
                return (key, value)
            })
        case "forall", "exists":
            return .object(pairs.map { key, value in
                if key == "body" {
                    return (key, normalizeFormulaForVerify(value))
                }
                return (key, value)
            })
        default:
            return formula
        }
    }

    private static func normalizeTermForVerify(_ term: JcsCanonical) -> JcsCanonical {
        guard case .object(let pairs) = term,
              let kind = pairs.first(where: { $0.0 == "kind" })?.1.stringValue
        else {
            return term
        }

        switch kind {
        case "var":
            return .object(pairs.map { key, value in
                if key == "name", value.stringValue == "return_value" {
                    return (key, .string("result"))
                }
                return (key, value)
            })
        case "const":
            return term
        case "ctor":
            let name = field("name", in: term).stringValue ?? ""
            let args = field("args", in: term).arrayValue ?? []
            if name == "swift:return", args.count == 1 {
                return normalizeTermForVerify(args[0])
            }
            return .object(pairs.map { key, value in
                if key == "name", let name = value.stringValue {
                    return (key, .string(normalizeCtorName(name)))
                }
                if key == "args", case .array(let args) = value {
                    return (key, .array(args.map(normalizeTermForVerify)))
                }
                return (key, value)
            })
        default:
            return term
        }
    }

    private static func normalizeCtorName(_ name: String) -> String {
        switch name {
        case "swift:add": return "+"
        case "swift:sub": return "-"
        case "swift:mul": return "*"
        default: return name
        }
    }

    private static func normalizeAtomicName(_ name: String) -> String {
        switch name {
        case "swift:eq": return "="
        case "swift:ne": return "≠"
        case "swift:lt": return "<"
        case "swift:le": return "≤"
        case "swift:gt": return ">"
        case "swift:ge": return "≥"
        default: return name
        }
    }

    private static func simpleFunctionSymbol(_ fnName: String) -> String {
        let withoutParams = fnName.split(separator: "(", maxSplits: 1).first.map(String.init) ?? fnName
        return withoutParams.split(separator: ".").last.map(String.init) ?? withoutParams
    }

    private static func field(_ name: String, in object: JcsCanonical) -> JcsCanonical {
        guard case .object(let pairs) = object else {
            return .null
        }
        return pairs.first(where: { $0.0 == name })?.1 ?? .null
    }

    public static func canonicalBytes(_ value: JcsCanonical) -> Data {
        JcsCanonicalizer.encode(value)
    }

    public static func anyValue(_ value: JcsCanonical) -> Any {
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

    public static func canonical(from value: Any) throws -> JcsCanonical {
        if value is NSNull {
            return .null
        }
        if let bool = value as? Bool {
            return .bool(bool)
        }
        if let int = value as? Int {
            return .int(Int64(int))
        }
        if let int64 = value as? Int64 {
            return .int(int64)
        }
        if let number = value as? NSNumber {
            let objCType = String(cString: number.objCType)
            if objCType == "c" || objCType == "B" {
                return .bool(number.boolValue)
            }
            return .int(number.int64Value)
        }
        if let string = value as? String {
            return .string(string)
        }
        if let array = value as? [Any] {
            return .array(try array.map(canonical(from:)))
        }
        if let object = value as? [String: Any] {
            return .object(try object.keys.sorted().map { key in
                (key, try canonical(from: object[key] as Any))
            })
        }
        throw SwiftSourceError.invalidIR("unsupported JSON value: \(type(of: value))")
    }

    private static func const(value: JcsCanonical, sort: String) -> JcsCanonical {
        .object([
            ("kind", .string("const")),
            ("sort", primitiveSort(sort)),
            ("value", value),
        ])
    }
}

private extension JcsCanonical {
    var stringValue: String? {
        if case .string(let value) = self {
            return value
        }
        return nil
    }

    var arrayValue: [JcsCanonical]? {
        if case .array(let values) = self {
            return values
        }
        return nil
    }
}

enum SwiftEffect: Hashable {
    case reads(String)
    case writes(String)
    case io
    case unsafe
    case panics
    case unresolvedCall(String)
    case opaqueLoop(String)

    var sortKey: String {
        switch self {
        case .reads(let target): return "0:reads:\(target)"
        case .writes(let target): return "1:writes:\(target)"
        case .io: return "2:io"
        case .unsafe: return "3:unsafe"
        case .panics: return "4:panics"
        case .unresolvedCall(let name): return "5:unresolved:\(name)"
        case .opaqueLoop(let loopCid): return "6:opaque_loop:\(loopCid)"
        }
    }

    var value: JcsCanonical {
        switch self {
        case .reads(let target):
            return .object([("kind", .string("reads")), ("target", .string(target))])
        case .writes(let target):
            return .object([("kind", .string("writes")), ("target", .string(target))])
        case .io:
            return .object([("kind", .string("io"))])
        case .unsafe:
            return .object([("kind", .string("unsafe"))])
        case .panics:
            return .object([("kind", .string("panics"))])
        case .unresolvedCall(let name):
            return .object([("kind", .string("unresolved_call")), ("name", .string(name))])
        case .opaqueLoop(let loopCid):
            return .object([("kind", .string("opaque_loop")), ("loopCid", .string(loopCid))])
        }
    }
}

struct SwiftEffectSet {
    private var effects: Set<SwiftEffect> = []

    mutating func add(_ effect: SwiftEffect) {
        effects.insert(effect)
    }

    func sortedValues() -> [JcsCanonical] {
        effects.sorted { $0.sortKey < $1.sortKey }.map(\.value)
    }
}
