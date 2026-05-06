// SPDX-License-Identifier: Apache-2.0
//
// ProvekIt IR types + JCS canonical JSON emitter for Swift.
//
// Mirrors the Rust, Go, Java, Python, C++, C, Zig, Ruby, C#, and
// TypeScript kits. Uses Swift enums with associated values for
// tagged unions, and a custom JSON encoder for JCS key ordering.

import Foundation
import ProvekitCrypto

// MARK: - Sort

public indirect enum Sort: Equatable, Hashable, Sendable {
    case primitive(String)
    case function(args: [Sort], return_: Sort)
    case dependent(name: String, indexVar: String, indexSort: Sort)
    case region(name: String)

    public static let int = Sort.primitive("Int")
    public static let real = Sort.primitive("Real")
    public static let string = Sort.primitive("String")
    public static let bool = Sort.primitive("Bool")
    public static let ref = Sort.primitive("Ref")
    public static let node = Sort.primitive("Node")
}

// MARK: - Term

public enum Term: Equatable, Hashable {
    case `var`(name: String)
    case const(value: ConstValue, sort: Sort)
    case ctor(name: String, args: [Term])

    public static func num(_ n: Int64) -> Term { .const(value: .int(n), sort: .int) }
    public static func str(_ s: String) -> Term { .const(value: .string(s), sort: .string) }
    public static func bool(_ b: Bool) -> Term { .const(value: .bool(b), sort: .bool) }
    public static func nullRef() -> Term { .const(value: .null, sort: .ref) }
    public static func ctor(_ name: String, _ args: Term...) -> Term {
        .ctor(name: name, args: args)
    }
}

public enum ConstValue: Equatable, Hashable {
    case int(Int64)
    case string(String)
    case bool(Bool)
    case null
}

// MARK: - Formula

public indirect enum Formula: Equatable, Hashable {
    case atomic(name: String, args: [Term])
    case connective(kind: String, operands: [Formula])
    case quantifier(kind: String, name: String, sort: Sort, body: Formula)

    public static func eq(_ a: Term, _ b: Term) -> Formula {
        .atomic(name: "=", args: [a, b])
    }
    public static func neq(_ a: Term, _ b: Term) -> Formula {
        .atomic(name: "≠", args: [a, b])
    }
    public static func gte(_ a: Term, _ b: Term) -> Formula {
        .atomic(name: "≥", args: [a, b])
    }
    public static func lte(_ a: Term, _ b: Term) -> Formula {
        .atomic(name: "≤", args: [a, b])
    }
    public static func gt(_ a: Term, _ b: Term) -> Formula {
        .atomic(name: ">", args: [a, b])
    }
    public static func lt(_ a: Term, _ b: Term) -> Formula {
        .atomic(name: "<", args: [a, b])
    }
    public static func and(_ operands: Formula...) -> Formula {
        .connective(kind: "and", operands: operands)
    }
    public static func or(_ operands: Formula...) -> Formula {
        .connective(kind: "or", operands: operands)
    }
    public static func implies(_ a: Formula, _ b: Formula) -> Formula {
        .connective(kind: "implies", operands: [a, b])
    }
    public static func not(_ a: Formula) -> Formula {
        .connective(kind: "not", operands: [a])
    }
    public static func forall(name: String, sort: Sort, body: Formula) -> Formula {
        .quantifier(kind: "forall", name: name, sort: sort, body: body)
    }
}

// MARK: - Declaration

public indirect enum Declaration: Equatable, Hashable {
    case contract(name: String, outBinding: String, pre: Formula?, post: Formula?, inv: Formula?)
    case bridge(name: String, sourceSymbol: String, sourceLayer: String,
                sourceContractCid: String, targetContractCid: String,
                targetProofCid: String, targetLayer: String, notes: String?)
}

// MARK: - JCS Value tree

public enum JcsValue {
    case string(String)
    case number(String)
    case bool(Bool)
    case null
    case object([(String, JcsValue)])
    case array([JcsValue])

    static func obj(_ pairs: (String, JcsValue)...) -> JcsValue {
        .object(pairs)
    }
    static func arr(_ vals: [JcsValue]) -> JcsValue {
        .array(vals)
    }
}

// MARK: - JCS Canonical Emitter

public enum Jcs {

    public static func encode(_ value: JcsValue) -> String {
        switch value {
        case .string(let s):
            return encodeString(s)
        case .number(let n):
            return n
        case .bool(let b):
            return b ? "true" : "false"
        case .null:
            return "null"
        case .array(let arr):
            return "[\(arr.map { encode($0) }.joined(separator: ","))]"
        case .object(let pairs):
            let sorted = pairs.sorted { $0.0 < $1.0 }
            let items = sorted.map { "\(encodeString($0.0)):\(encode($0.1))" }
            return "{\(items.joined(separator: ","))}"
        }
    }

    // MARK: Term → JcsValue

    public static func termToValue(_ t: Term) -> JcsValue {
        switch t {
        case .var(let name):
            return .obj(("kind", .string("var")), ("name", .string(name)))
        case .const(let val, let sort):
            return .obj(("kind", .string("const")),
                        ("sort", sortToValue(sort)),
                        ("value", constValToValue(val)))
        case .ctor(let name, let args):
            return .obj(("args", .arr(args.map(termToValue))),
                        ("kind", .string("ctor")),
                        ("name", .string(name)))
        }
    }

    // MARK: Formula → JcsValue

    public static func formulaToValue(_ f: Formula) -> JcsValue {
        switch f {
        case .atomic(let name, let args):
            return .obj(("args", .arr(args.map(termToValue))),
                        ("kind", .string("atomic")),
                        ("name", .string(name)))
        case .connective(let kind, let ops):
            return .obj(("kind", .string(kind)),
                        ("operands", .arr(ops.map(formulaToValue))))
        case .quantifier(let kind, let name, let sort, let body):
            return .obj(("body", formulaToValue(body)),
                        ("kind", .string(kind)),
                        ("name", .string(name)),
                        ("sort", sortToValue(sort)))
        }
    }

    // MARK: Declaration → JcsValue → JSON

    public static func encodeDeclarations(_ decls: [Declaration]) -> String {
        let arr = decls.map(declToValue)
        return encode(.array(arr))
    }

    public static func declToValue(_ d: Declaration) -> JcsValue {
        switch d {
        case .contract(let name, let outBinding, let pre, let post, let inv):
            var pairs: [(String, JcsValue)] = []
            if let inv = inv { pairs.append(("inv", formulaToValue(inv))) }
            pairs.append(("kind", .string("contract")))
            pairs.append(("name", .string(name)))
            pairs.append(("outBinding", .string(outBinding)))
            if let post = post { pairs.append(("post", formulaToValue(post))) }
            if let pre = pre { pairs.append(("pre", formulaToValue(pre))) }
            return .object(pairs)
        case .bridge(let name, let sourceSymbol, let sourceLayer,
                     let sourceContractCid, let targetContractCid,
                     let targetProofCid, let targetLayer, let notes):
            var pairs: [(String, JcsValue)] = [
                ("kind", .string("bridge")),
                ("name", .string(name)),
                ("sourceContractCid", .string(sourceContractCid)),
                ("sourceLayer", .string(sourceLayer)),
                ("sourceSymbol", .string(sourceSymbol)),
                ("targetContractCid", .string(targetContractCid)),
                ("targetLayer", .string(targetLayer)),
                ("targetProofCid", .string(targetProofCid)),
            ]
            if let notes = notes { pairs.append(("notes", .string(notes))) }
            return .object(pairs)
        }
    }

    // Helpers

    static func sortToValue(_ s: Sort) -> JcsValue {
        switch s {
        case .primitive(let name):
            return .obj(("kind", .string("primitive")), ("name", .string(name)))
        case .function(let args, let return_):
            return .obj(("args", .arr(args.map(sortToValue))),
                        ("kind", .string("function")),
                        ("return", sortToValue(return_)))
        case .dependent(let name, let indexVar, let indexSort):
            return .obj(("indexSort", sortToValue(indexSort)),
                        ("indexVar", .string(indexVar)),
                        ("kind", .string("dependent")),
                        ("name", .string(name)))
        case .region(let name):
            return .obj(("kind", .string("region")), ("name", .string(name)))
        }
    }

    static func constValToValue(_ cv: ConstValue) -> JcsValue {
        switch cv {
        case .int(let n): return .number(String(n))
        case .string(let s): return .string(s)
        case .bool(let b): return .bool(b)
        case .null: return .null
        }
    }

    static func encodeString(_ s: String) -> String {
        var out = "\""
        for c in s.unicodeScalars {
            switch c.value {
            case 0x0022: out += "\\\""
            case 0x005C: out += "\\\\"
            case 0..<0x20: out += String(format: "\\u00%02x", c.value)
            default: out.append(Character(c))
            }
        }
        out += "\""
        return out
    }
}

// BLAKE3 — delegates to the native pure-Swift / vendored-C implementation
// in ProvekitCrypto. The legacy python-shellout has been retired; output
// is byte-equivalent to the previous shell-out behavior because both
// ultimately call the BLAKE3 reference implementation.
//
// This typealias preserves the existing module-local API surface
// (`Provekit.Blake3.hex(_:)`) used by CrossKitBridges, ConformanceRunner,
// and MintSwiftSelfContracts. New code should `import ProvekitCrypto`
// directly and use `ProvekitCrypto.Blake3`.
public typealias Blake3 = ProvekitCrypto.Blake3
