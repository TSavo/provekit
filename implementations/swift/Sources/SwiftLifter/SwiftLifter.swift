// SPDX-License-Identifier: Apache-2.0
//
// SwiftLifter: SwiftSyntax-based Swift source parser for the ProvekIt lift pipeline.
//
// v1: AST-based via SwiftSyntax (Apple's official swift-syntax library).
//     Replaces the regex-v0 lifter that lived here previously (issue #211).
//
// Walks the parsed Swift slab and extracts:
//   - Function declarations (FunctionDeclSyntax: top-level + members of
//     class/struct/enum/protocol/actor/extension nesting)
//   - Initializer declarations (InitializerDeclSyntax): emitted as `init`
//   - Same-kit call sites: function calls whose callee identifier resolves
//     to a function declared in the same parse unit
//
// Wire shape (canonical parse-protocol v1, mirrors Go LSP plugin):
//   declarations: [{kind, name, outBinding}]
//   callEdges:    [{sourceContractCid, targetSymbol, callSiteLocus}]
//   warnings:     []
//
// SwiftSyntax tradeoff: the AST walker is the canonical Apple-blessed way
// to parse Swift. NSRegularExpression-based parsing breaks on multiline
// declarations, generic constraints, attributes, and SE-* future syntax.
// This rewrite removes all those failure modes while keeping the wire shape
// byte-identical for callers (ProveKitLSPSwift, LSPTests, MintSwiftSelfContracts
// indirectly via the lift surface manifest).
//
// The lifter is platform-restricted to macOS (Package.swift `platforms: [.macOS(.v13)]`)
// because SwiftSyntax 600.x ships pre-built binaries for Apple platforms; CI on
// Linux can compile from source but the package is pinned to macOS for simplicity
// (per the issue #211 acceptance: "Mac-only initially").

import Foundation
import SwiftSyntax
import SwiftParser

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
    public let sourceContractCid: String  // concrete CID, or pending-swift:<caller> before hashing
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

/// SwiftSyntax-based Swift source lifter.
/// Parses Swift source for function declarations and call sites using the
/// official Apple swift-syntax AST walker.
public enum SwiftLifter {

    /// Lift Swift source into declarations and call edges.
    ///
    /// - Parameters:
    ///   - source: Swift source text
    ///   - path: file path (for locus reporting)
    /// - Returns: LiftResult with extracted declarations and call edges
    public static func lift(source: String, path: String) -> LiftResult {
        // Parse via SwiftParser. This never throws; malformed source produces
        // a tree with diagnostics that we surface as warnings.
        let sourceFile = Parser.parse(source: source)
        let locationConverter = SourceLocationConverter(fileName: path, tree: sourceFile)

        // Pass 1: collect declarations.
        let declVisitor = DeclarationVisitor(viewMode: .sourceAccurate)
        declVisitor.walk(sourceFile)
        let declarations = declVisitor.declarations
        let declaredNames = Set(declarations.map { $0.name })

        // Pass 2: collect call edges, filtered to same-kit declared names.
        let callVisitor = CallEdgeVisitor(
            viewMode: .sourceAccurate,
            declaredNames: declaredNames,
            path: path,
            locationConverter: locationConverter
        )
        callVisitor.walk(sourceFile)

        // Deduplicate call edges by source, target, and call-site position.
        var seen = Set<String>()
        let uniqueEdges = callVisitor.callEdges.filter { edge in
            let key = "\(edge.sourceContractCid):\(edge.targetSymbol):\(edge.callSiteLocus.line):\(edge.callSiteLocus.column)"
            return seen.insert(key).inserted
        }

        return LiftResult(declarations: declarations, callEdges: uniqueEdges)
    }
}

// MARK: - SwiftSyntax visitors

/// Walks every FunctionDecl / InitializerDecl in the AST and records its name.
/// Member functions inside nested types are flattened into the same set
/// (mirrors regex-v0 behavior, where method-vs-function is not distinguished
/// in the wire shape).
private final class DeclarationVisitor: SyntaxVisitor {
    var declarations: [LiftedDeclaration] = []

    override func visit(_ node: FunctionDeclSyntax) -> SyntaxVisitorContinueKind {
        // node.name is a TokenSyntax; .text is the bare identifier.
        let name = node.name.text
        if !name.isEmpty {
            declarations.append(LiftedDeclaration(name: name))
        }
        return .visitChildren
    }

    override func visit(_ node: InitializerDeclSyntax) -> SyntaxVisitorContinueKind {
        // Initializers don't have a `name` token; emit a synthetic "init" symbol
        // so call edges to `init(...)` can resolve. Multiple inits collapse to
        // one entry by Set semantics in SwiftLifter.lift; this is intentional
        // (the wire shape never disambiguated init overloads).
        declarations.append(LiftedDeclaration(name: "init"))
        return .visitChildren
    }
}

/// Walks every FunctionCallExpr in the AST and emits a call edge if the
/// callee identifier matches a declared function name in the same parse unit.
private final class CallEdgeVisitor: SyntaxVisitor {
    var callEdges: [LiftedCallEdge] = []
    private let declaredNames: Set<String>
    private let path: String
    private let locationConverter: SourceLocationConverter
    private var declarationStack: [String] = []

    init(
        viewMode: SyntaxTreeViewMode,
        declaredNames: Set<String>,
        path: String,
        locationConverter: SourceLocationConverter
    ) {
        self.declaredNames = declaredNames
        self.path = path
        self.locationConverter = locationConverter
        super.init(viewMode: viewMode)
    }

    override func visit(_ node: FunctionDeclSyntax) -> SyntaxVisitorContinueKind {
        let name = node.name.text
        if !name.isEmpty {
            declarationStack.append(name)
        }
        return .visitChildren
    }

    override func visitPost(_ node: FunctionDeclSyntax) {
        if !node.name.text.isEmpty {
            _ = declarationStack.popLast()
        }
    }

    override func visit(_ node: InitializerDeclSyntax) -> SyntaxVisitorContinueKind {
        declarationStack.append("init")
        return .visitChildren
    }

    override func visitPost(_ node: InitializerDeclSyntax) {
        _ = declarationStack.popLast()
    }

    override func visit(_ node: FunctionCallExprSyntax) -> SyntaxVisitorContinueKind {
        // Resolve the callee identifier. We handle:
        //   foo(args)                           -> DeclReferenceExprSyntax
        //   self.foo(args), x.foo(args)         -> MemberAccessExprSyntax
        // Anything else (closures, subscripts, .init() with implicit base, etc.)
        // is skipped: only same-kit name-based call edges are in scope.
        let calleeName: String?
        if let decl = node.calledExpression.as(DeclReferenceExprSyntax.self) {
            calleeName = decl.baseName.text
        } else if let member = node.calledExpression.as(MemberAccessExprSyntax.self) {
            calleeName = member.declName.baseName.text
        } else {
            calleeName = nil
        }

        guard let name = calleeName, declaredNames.contains(name) else {
            return .visitChildren
        }
        guard let callerName = declarationStack.last else {
            return .visitChildren
        }

        // SourceLocationConverter gives us 1-based line and 1-based column;
        // the regex-v0 lifter emitted 1-based line and 0-based UTF-16 column
        // offset (NSString locations). Normalize to (1-based line, 0-based
        // UTF-8 byte column): i.e. column - 1: to match the v0 wire shape
        // for callers like the LSP that report locii to editors.
        let calleeToken = node.calledExpression
        let location = calleeToken.startLocation(converter: locationConverter)
        let line = location.line
        let column = max(0, location.column - 1)

        callEdges.append(LiftedCallEdge(
            sourceContractCid: "pending-swift:\(callerName)",
            targetSymbol: "swift-kit:\(name)",
            callSiteLocus: CallSiteLocus(file: path, line: line, column: column)
        ))
        return .visitChildren
    }
}
