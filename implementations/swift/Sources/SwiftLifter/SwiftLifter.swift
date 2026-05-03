// SPDX-License-Identifier: Apache-2.0
//
// SwiftLifter — regex-based Swift source parser for the ProvekIt lift pipeline.
//
// v0: regex-based. Does not depend on SwiftSyntax. AST-based parsing via
// SwiftSyntax is deferred to a follow-up PR once the Swift toolchain
// dependency situation stabilizes across CI environments.
//
// Extracts:
//   - Top-level and member function declarations (func keyword)
//   - Call sites (simple name-call patterns)
//
// Wire shape (canonical parse-protocol v1):
//   declarations: [{kind, name, outBinding}]
//   callEdges:    [{sourceContractCid, targetSymbol, callSiteLocus}]
//   warnings:     []
//
// Corresponds to the Go LSP plugin's parse result shape
// (implementations/go/cmd/provekit-lsp-go/main.go).

import Foundation

// MARK: - Wire shapes

/// A lifted declaration object in parse-protocol wire shape.
public struct LiftedDeclaration: Sendable {
    public let kind: String       // "contract"
    public let name: String
    public let outBinding: String // always "out" per protocol spec

    public init(kind: String = "contract", name: String, outBinding: String = "out") {
        self.kind = kind
        self.name = name
        self.outBinding = outBinding
    }

    public func toDict() -> [String: Any] {
        return ["kind": kind, "name": name, "outBinding": outBinding]
    }
}

/// A call-edge object in parse-protocol wire shape.
public struct LiftedCallEdge: Sendable {
    public let sourceContractCid: String  // placeholder: empty when CID unknown
    public let targetSymbol: String
    public let callSiteLocus: CallSiteLocus

    public init(sourceContractCid: String = "", targetSymbol: String, callSiteLocus: CallSiteLocus) {
        self.sourceContractCid = sourceContractCid
        self.targetSymbol = targetSymbol
        self.callSiteLocus = callSiteLocus
    }

    public func toDict() -> [String: Any] {
        return [
            "sourceContractCid": sourceContractCid,
            "targetSymbol": targetSymbol,
            "callSiteLocus": callSiteLocus.toDict(),
        ]
    }
}

public struct CallSiteLocus: Sendable {
    public let file: String
    public let line: Int
    public let column: Int

    public init(file: String, line: Int, column: Int) {
        self.file = file
        self.line = line
        self.column = column
    }

    public func toDict() -> [String: Any] {
        return ["file": file, "line": line, "column": column]
    }
}

// MARK: - Lift result

public struct LiftResult: Sendable {
    public let declarations: [LiftedDeclaration]
    public let callEdges: [LiftedCallEdge]
    public let warnings: [String]

    public init(declarations: [LiftedDeclaration], callEdges: [LiftedCallEdge], warnings: [String] = []) {
        self.declarations = declarations
        self.callEdges = callEdges
        self.warnings = warnings
    }
}

// MARK: - SwiftLifter

/// Regex-based Swift source lifter.
/// Parses Swift source for function declarations and call sites.
public enum SwiftLifter {

    // Patterns (non-concurrent value types used inside nonisolated context).
    // Swift 6: NSRegularExpression is not Sendable; keep in a nonisolated helper.

    /// Extract top-level and member function declarations.
    /// Matches:
    ///   func name(params) -> ReturnType
    ///   func name(params)  (no return type)
    ///   override/public/private/internal/static/class/open prefixed variants
    static let funcPattern =
        #"(?:(?:override|public|private|internal|fileprivate|static|class|open|final|mutating|nonmutating|required|optional|dynamic|prefix|postfix|infix)\s+)*func\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*(?:<[^>]*>)?\s*\("#

    /// Extract simple call sites: identifier followed by `(`.
    /// Excludes `func`, `if`, `while`, `for`, `switch`, `guard`, `return`, `class`,
    /// `struct`, `enum`, `protocol`, `import`, `var`, `let`, `init`.
    static let callPattern =
        #"\b([a-zA-Z_][a-zA-Z0-9_]*)\s*\("#

    static let reservedKeywords: Set<String> = [
        "func", "if", "while", "for", "switch", "guard", "return", "class",
        "struct", "enum", "protocol", "import", "var", "let", "init", "super",
        "self", "true", "false", "nil", "catch", "throw", "throws", "do",
        "try", "defer", "repeat", "in", "else", "where", "case", "default",
        "break", "continue", "fallthrough", "as", "is", "typealias", "extension",
        "static", "override", "public", "private", "internal", "fileprivate",
        "open", "final", "lazy", "weak", "unowned", "required", "optional",
        "dynamic", "mutating", "nonmutating", "indirect", "subscript",
        "operator", "prefix", "postfix", "infix", "associativity",
        "precedence", "import", "module", "package", "nonisolated", "actor",
        "async", "await", "rethrows", "some", "any", "print", "debugPrint",
        "assert", "precondition", "fatalError", "assertionFailure",
    ]

    /// Lift Swift source into declarations and call edges.
    ///
    /// - Parameters:
    ///   - source: Swift source text
    ///   - path: file path (for locus reporting)
    /// - Returns: LiftResult with extracted declarations and call edges
    public static func lift(source: String, path: String) -> LiftResult {
        let lines = source.components(separatedBy: "\n")
        var declarations: [LiftedDeclaration] = []
        var callEdges: [LiftedCallEdge] = []

        guard let funcRegex = try? NSRegularExpression(pattern: funcPattern),
              let callRegex = try? NSRegularExpression(pattern: callPattern)
        else {
            return LiftResult(declarations: [], callEdges: [])
        }

        var declaredNames: Set<String> = []

        // First pass: collect declarations.
        for (lineIdx, line) in lines.enumerated() {
            let nsLine = line as NSString
            let range = NSRange(location: 0, length: nsLine.length)
            let matches = funcRegex.matches(in: line, range: range)
            for match in matches {
                if match.numberOfRanges >= 2 {
                    let nameRange = match.range(at: 1)
                    if nameRange.location != NSNotFound {
                        let name = nsLine.substring(with: nameRange)
                        if !reservedKeywords.contains(name) {
                            declaredNames.insert(name)
                            declarations.append(LiftedDeclaration(name: name))
                        }
                    }
                }
                _ = lineIdx  // suppress unused warning
            }
        }

        // Second pass: collect call edges from all non-comment lines.
        // On lines that declare functions, the func name itself is excluded
        // (it's a declaration, not a call site). Other calls on the same
        // line (e.g. `func compute() -> Int { return add(1, 2) }`) ARE
        // emitted as call edges.
        for (lineIdx, line) in lines.enumerated() {
            // Skip blank lines and comment lines.
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if trimmed.isEmpty || trimmed.hasPrefix("//") || trimmed.hasPrefix("/*") || trimmed.hasPrefix("*") {
                continue
            }

            let nsLine = line as NSString
            let lineRange = NSRange(location: 0, length: nsLine.length)

            // Collect the ranges of func declaration names so we can exclude them.
            var declNameRanges: [NSRange] = []
            let funcMatches = funcRegex.matches(in: line, range: lineRange)
            for fm in funcMatches {
                if fm.numberOfRanges >= 2 {
                    let nr = fm.range(at: 1)
                    if nr.location != NSNotFound {
                        declNameRanges.append(nr)
                    }
                }
            }

            // Find call patterns.
            let callMatches = callRegex.matches(in: line, range: lineRange)
            for match in callMatches {
                if match.numberOfRanges >= 2 {
                    let nameRange = match.range(at: 1)
                    if nameRange.location != NSNotFound {
                        // Skip if this match is the func declaration name itself.
                        let isDeclName = declNameRanges.contains { $0 == nameRange }
                        if isDeclName { continue }

                        let callee = nsLine.substring(with: nameRange)
                        // Only emit call edges for known declared functions
                        // (same-kit calls per Go LSP pattern).
                        if declaredNames.contains(callee) && !reservedKeywords.contains(callee) {
                            let colOffset = nameRange.location
                            callEdges.append(LiftedCallEdge(
                                sourceContractCid: "",
                                targetSymbol: callee,
                                callSiteLocus: CallSiteLocus(
                                    file: path,
                                    line: lineIdx + 1,
                                    column: colOffset
                                )
                            ))
                        }
                    }
                }
            }
        }

        // Deduplicate call edges by (targetSymbol, line, column).
        var seen = Set<String>()
        let uniqueEdges = callEdges.filter { edge in
            let key = "\(edge.targetSymbol):\(edge.callSiteLocus.line):\(edge.callSiteLocus.column)"
            return seen.insert(key).inserted
        }

        return LiftResult(declarations: declarations, callEdges: uniqueEdges)
    }
}
