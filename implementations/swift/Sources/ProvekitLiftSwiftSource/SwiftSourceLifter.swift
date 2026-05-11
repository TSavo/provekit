// SPDX-License-Identifier: Apache-2.0

import Foundation
import SwiftParser
import SwiftSyntax
import ProvekitCrypto

public enum SwiftSourceLifter {
    public static func liftSource(_ source: String, path: String) -> SwiftSourceLiftResult {
        let sourceFile = Parser.parse(source: source)
        let converter = SourceLocationConverter(fileName: path, tree: sourceFile)
        let moduleName = modulePath(from: path)

        let collector = SwiftDefinitionCollector(moduleName: moduleName)
        collector.walk(sourceFile)

        var result = SwiftSourceLiftResult()
        result.refusals.append(contentsOf: collector.collectorRefusals)
        var acceptedBodyTerms: [JcsCanonical] = []
        var contracts: [JcsCanonical] = []

        for function in collector.functions {
            do {
                let emitter = try SwiftFunctionEmitter(
                    function: function,
                    sourcePath: path,
                    converter: converter,
                    globalVars: collector.globalVars,
                    staticVars: collector.staticVars
                )
                let contract = try emitter.emit()
                acceptedBodyTerms.append(try SwiftSourceIR.bodyTerm(of: contract))
                contracts.append(contract)
            } catch let unsupported as UnsupportedSwiftSyntax {
                result.refusals.append(SwiftSourceIR.refusal(
                    kind: unsupported.kind,
                    function: function.fnName,
                    line: unsupported.line,
                    reason: unsupported.reason
                ))
            } catch {
                result.refusals.append(SwiftSourceIR.refusal(
                    kind: "analysis-error",
                    function: function.fnName,
                    line: function.line,
                    reason: String(describing: error)
                ))
            }
        }

        result.ir.append(SwiftSourceIR.sourceUnitContract(
            sourcePath: path,
            source: source,
            operationalTerm: SwiftSourceIR.foldSeq(acceptedBodyTerms)
        ))
        result.ir.append(contentsOf: contracts)
        return result
    }

    public static func liftPaths(workspaceRoot: String, sourcePaths: [String]) -> SwiftSourceLiftResult {
        var result = SwiftSourceLiftResult()
        let root = URL(fileURLWithPath: workspaceRoot.isEmpty ? "." : workspaceRoot).standardizedFileURL
        let rootPath = root.path
        let fileManager = FileManager.default

        for requested in sourcePaths {
            let requestedURL = URL(fileURLWithPath: requested, relativeTo: root).standardizedFileURL
            let fullPath = requestedURL.path
            guard isPath(fullPath, inside: rootPath) else {
                result.refusals.append(SwiftSourceIR.refusal(
                    kind: "path-traversal",
                    function: nil,
                    line: nil,
                    reason: "path '\(requested)' escapes workspace root '\(rootPath)'"
                ))
                continue
            }

            var isDirectory: ObjCBool = false
            guard fileManager.fileExists(atPath: fullPath, isDirectory: &isDirectory) else {
                result.diagnostics.append(.object([
                    ("severity", .string("warning")),
                    ("message", .string("path not found: \(fullPath)")),
                ]))
                continue
            }

            let files: [String]
            if isDirectory.boolValue {
                let enumerator = fileManager.enumerator(atPath: fullPath)
                files = (enumerator?.compactMap { item -> String? in
                    guard let rel = item as? String, rel.hasSuffix(".swift") else {
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
                    let fileResult = liftSource(source, path: displayPath)
                    result.ir.append(contentsOf: fileResult.ir)
                    result.diagnostics.append(contentsOf: fileResult.diagnostics)
                    result.opacityReport.append(contentsOf: fileResult.opacityReport)
                    result.refusals.append(contentsOf: fileResult.refusals)
                } catch {
                    result.refusals.append(SwiftSourceIR.refusal(
                        kind: "io-error",
                        function: nil,
                        line: nil,
                        reason: "cannot read '\(file)': \(error)"
                    ))
                }
            }
        }
        return result
    }
}

private struct SwiftFunctionInfo {
    let node: FunctionDeclSyntax
    let fnName: String
    let formals: [String]
    let formalSorts: [String]
    let returnSort: String
    let line: Int
}

private struct UnsupportedSwiftSyntax: Error {
    let kind: String
    let line: Int?
    let reason: String
}

private final class SwiftDefinitionCollector: SyntaxVisitor {
    private let moduleName: String
    private var typeScope: [String] = []
    private var converter: SourceLocationConverter?

    var functions: [SwiftFunctionInfo] = []
    var collectorRefusals: [JcsCanonical] = []
    var globalVars: Set<String> = []
    var staticVars: Set<String> = []

    init(moduleName: String) {
        self.moduleName = moduleName
        super.init(viewMode: .sourceAccurate)
    }

    override func visit(_ node: SourceFileSyntax) -> SyntaxVisitorContinueKind {
        converter = SourceLocationConverter(fileName: moduleName, tree: node)
        return .visitChildren
    }

    override func visit(_ node: StructDeclSyntax) -> SyntaxVisitorContinueKind {
        typeScope.append(node.name.text)
        return .visitChildren
    }

    override func visitPost(_ node: StructDeclSyntax) {
        _ = typeScope.popLast()
    }

    override func visit(_ node: ClassDeclSyntax) -> SyntaxVisitorContinueKind {
        typeScope.append(node.name.text)
        return .visitChildren
    }

    override func visitPost(_ node: ClassDeclSyntax) {
        _ = typeScope.popLast()
    }

    override func visit(_ node: EnumDeclSyntax) -> SyntaxVisitorContinueKind {
        typeScope.append(node.name.text)
        return .visitChildren
    }

    override func visitPost(_ node: EnumDeclSyntax) {
        _ = typeScope.popLast()
    }

    override func visit(_ node: ActorDeclSyntax) -> SyntaxVisitorContinueKind {
        typeScope.append(node.name.text)
        return .visitChildren
    }

    override func visitPost(_ node: ActorDeclSyntax) {
        _ = typeScope.popLast()
    }

    override func visit(_ node: ExtensionDeclSyntax) -> SyntaxVisitorContinueKind {
        typeScope.append(cleanTypeName(node.extendedType.description))
        return .visitChildren
    }

    override func visitPost(_ node: ExtensionDeclSyntax) {
        _ = typeScope.popLast()
    }

    override func visit(_ node: VariableDeclSyntax) -> SyntaxVisitorContinueKind {
        let names = node.bindings.compactMap { binding in
            binding.pattern.as(IdentifierPatternSyntax.self)?.identifier.text
        }
        if typeScope.isEmpty {
            for name in names {
                globalVars.insert(name)
            }
        } else if node.modifiers.description.contains("static")
                    || node.modifiers.description.contains("class") {
            let prefix = ([moduleName] + typeScope).joined(separator: ".")
            for name in names {
                staticVars.insert("\(prefix).\(name)")
            }
        }
        return .skipChildren
    }

    override func visit(_ node: FunctionDeclSyntax) -> SyntaxVisitorContinueKind {
        do {
            let info = try functionInfo(for: node)
            functions.append(info)
        } catch let unsupported as UnsupportedSwiftSyntax {
            let bestEffortName = node.name.text
            collectorRefusals.append(SwiftSourceIR.refusal(
                kind: unsupported.kind,
                function: bestEffortName,
                line: unsupported.line,
                reason: unsupported.reason
            ))
        } catch {
            let line: Int? = converter.map { node.startLocation(converter: $0).line }
            collectorRefusals.append(SwiftSourceIR.refusal(
                kind: "unparseable-function-signature",
                function: node.name.text,
                line: line,
                reason: String(describing: error)
            ))
        }
        return .skipChildren
    }

    private func functionInfo(for node: FunctionDeclSyntax) throws -> SwiftFunctionInfo {
        let params = Array(node.signature.parameterClause.parameters)
        let labels = params.map { parameter in
            parameter.firstName.text + ":"
        }.joined()
        let typeList = params.map { cleanTypeName($0.type.description) }
        let formals = params.map { parameter in
            if let second = parameter.secondName {
                return second.text
            }
            return parameter.firstName.text == "_" ? "_" : parameter.firstName.text
        }
        let returnSort = cleanTypeName(node.signature.returnClause?.type.description ?? "Void")
        let prefix = ([moduleName] + typeScope).joined(separator: ".")
        let fnName = "\(prefix).\(node.name.text)(\(labels))(\(typeList.joined(separator: ",")))->\(returnSort)"
        let line: Int
        if let converter {
            line = node.startLocation(converter: converter).line
        } else {
            line = 1
        }
        return SwiftFunctionInfo(
            node: node,
            fnName: fnName,
            formals: formals,
            formalSorts: typeList,
            returnSort: returnSort,
            line: line
        )
    }
}

private final class SwiftFunctionEmitter {
    private let function: SwiftFunctionInfo
    private let sourcePath: String
    private let converter: SourceLocationConverter
    private let globalVars: Set<String>
    private let staticVars: Set<String>
    private var locals: Set<String>
    private var effects = SwiftEffectSet()

    init(
        function: SwiftFunctionInfo,
        sourcePath: String,
        converter: SourceLocationConverter,
        globalVars: Set<String>,
        staticVars: Set<String>
    ) throws {
        self.function = function
        self.sourcePath = sourcePath
        self.converter = converter
        self.globalVars = globalVars
        self.staticVars = staticVars
        self.locals = Set(function.formals)
        try Self.validateSignature(function.node, info: function, converter: converter)
    }

    func emit() throws -> JcsCanonical {
        guard let body = function.node.body else {
            throw unsupported(function.node, kind: "missing-body", reason: "function declarations without bodies are not supported")
        }
        let bodyTerm = try statements(body.statements)
        return SwiftSourceIR.functionContract(
            fnName: function.fnName,
            formals: function.formals,
            formalSorts: function.formalSorts,
            returnSort: function.returnSort,
            bodyTerm: bodyTerm,
            effects: effects.sortedValues(),
            sourcePath: sourcePath,
            line: function.line
        )
    }

    private static func validateSignature(
        _ node: FunctionDeclSyntax,
        info: SwiftFunctionInfo,
        converter: SourceLocationConverter
    ) throws {
        func fail(_ reason: String) throws -> Never {
            throw UnsupportedSwiftSyntax(
                kind: "unsupported-signature",
                line: node.startLocation(converter: converter).line,
                reason: reason
            )
        }

        if !node.attributes.isEmpty {
            try fail("function attributes are not supported")
        }
        if node.name.text.contains(where: { !$0.isLetter && !$0.isNumber && $0 != "_" }) {
            try fail("operator functions are not supported")
        }
        if node.genericParameterClause != nil || node.genericWhereClause != nil {
            try fail("generic functions are not supported")
        }
        let modifiers = node.modifiers.description
        if modifiers.contains("mutating") || modifiers.contains("nonmutating") {
            try fail("mutating methods are not supported")
        }
        if let effects = node.signature.effectSpecifiers?.description,
           effects.contains("async") || effects.contains("throws") || effects.contains("rethrows") {
            try fail("async/throws functions are not supported")
        }
        if !["Int", "Bool", "String"].contains(info.returnSort) {
            try fail("only scalar Int, Bool, and String return types are supported, got \(info.returnSort)")
        }
        for parameter in node.signature.parameterClause.parameters {
            let rendered = parameter.description
            if parameter.defaultValue != nil {
                try fail("default parameters are not supported")
            }
            if parameter.ellipsis != nil {
                try fail("variadic parameters are not supported")
            }
            if rendered.contains("inout") {
                try fail("inout parameters are not supported")
            }
            if rendered.contains("@autoclosure") || rendered.contains("@escaping") {
                try fail("@autoclosure and @escaping parameters are not supported")
            }
            let rawType = parameter.type.description.trimmingCharacters(in: .whitespacesAndNewlines)
            let type = cleanTypeName(rawType)
            if rawType.hasPrefix("some ") || rawType.hasPrefix("any ") || type.contains("<") {
                try fail("generic, existential, and opaque parameter types are not supported")
            }
            if !["Int", "Bool", "String"].contains(type) {
                try fail("only scalar Int, Bool, and String parameter types are supported, got \(type)")
            }
        }
    }

    private func statements(_ statements: CodeBlockItemListSyntax) throws -> JcsCanonical {
        var emitted: [JcsCanonical] = []
        for statement in statements {
            emitted.append(try codeBlockItem(statement))
        }
        return SwiftSourceIR.foldSeq(emitted)
    }

    private func codeBlockItem(_ item: CodeBlockItemSyntax) throws -> JcsCanonical {
        switch item.item {
        case .decl(let decl):
            if let variable = decl.as(VariableDeclSyntax.self) {
                return try variableDecl(variable)
            }
            throw unsupported(decl, reason: "unhandled declaration kind: \(type(of: decl))")
        case .stmt(let stmt):
            if let ret = stmt.as(ReturnStmtSyntax.self) {
                let expr = try ret.expression.map(expression) ?? SwiftSourceIR.unitConst()
                return SwiftSourceIR.ctor("swift:return", [expr])
            }
            if let ifExpr = stmt.as(IfExprSyntax.self) {
                return try ifTerm(ifExpr)
            }
            if let whileStmt = stmt.as(WhileStmtSyntax.self) {
                return try whileTerm(whileStmt)
            }
            if let repeatStmt = stmt.as(RepeatStmtSyntax.self) {
                return try repeatTerm(repeatStmt)
            }
            if let forStmt = stmt.as(ForStmtSyntax.self) {
                return try forTerm(forStmt)
            }
            if stmt.is(BreakStmtSyntax.self) {
                return SwiftSourceIR.ctor("swift:break", [SwiftSourceIR.unitConst()])
            }
            if stmt.is(ContinueStmtSyntax.self) {
                return SwiftSourceIR.ctor("swift:continue", [SwiftSourceIR.unitConst()])
            }
            if let throwStmt = stmt.as(ThrowStmtSyntax.self) {
                effects.add(.panics)
                let expr = try expression(throwStmt.expression)
                return SwiftSourceIR.ctor("swift:throw", [expr])
            }
            throw unsupported(stmt, reason: "unhandled statement kind: \(type(of: stmt))")
        case .expr(let expr):
            return try expression(expr)
        }
    }

    private func variableDecl(_ node: VariableDeclSyntax) throws -> JcsCanonical {
        if node.attributes.description.contains("@") {
            throw unsupported(node, kind: "unsupported-declaration", reason: "property wrappers and attributes are not supported")
        }

        var terms: [JcsCanonical] = []
        for binding in node.bindings {
            if binding.accessorBlock != nil {
                throw unsupported(binding, kind: "unsupported-declaration", reason: "computed properties are not supported")
            }
            guard let pattern = binding.pattern.as(IdentifierPatternSyntax.self) else {
                throw unsupported(binding, reason: "only identifier variable bindings are supported")
            }
            guard let initializer = binding.initializer else {
                throw unsupported(binding, kind: "unsupported-declaration", reason: "uninitialized declarations are not supported")
            }
            let name = pattern.identifier.text
            locals.insert(name)
            let value = try expression(initializer.value)
            terms.append(SwiftSourceIR.ctor("swift:assign", [
                SwiftSourceIR.varTerm(name),
                value,
            ]))
        }
        return SwiftSourceIR.foldSeq(terms)
    }

    private func ifTerm(_ node: IfExprSyntax) throws -> JcsCanonical {
        let cond = try conditionList(node.conditions)
        let thenBody = try statements(node.body.statements)
        let elseBody: JcsCanonical
        if let elseSyntax = node.elseBody {
            switch elseSyntax {
            case .codeBlock(let block):
                elseBody = try statements(block.statements)
            case .ifExpr(let nested):
                elseBody = try ifTerm(nested)
            }
        } else {
            elseBody = SwiftSourceIR.empty()
        }
        return SwiftSourceIR.ctor("swift:if", [cond, thenBody, elseBody])
    }

    private func whileTerm(_ node: WhileStmtSyntax) throws -> JcsCanonical {
        let loop = SwiftSourceIR.ctor("swift:while", [
            try conditionList(node.conditions),
            try statements(node.body.statements),
        ])
        effects.add(.opaqueLoop(computeJcsCid(loop)))
        return loop
    }

    private func repeatTerm(_ node: RepeatStmtSyntax) throws -> JcsCanonical {
        let loop = SwiftSourceIR.ctor("swift:repeat", [
            try statements(node.body.statements),
            try expression(node.condition),
        ])
        effects.add(.opaqueLoop(computeJcsCid(loop)))
        return loop
    }

    private func forTerm(_ node: ForStmtSyntax) throws -> JcsCanonical {
        guard node.whereClause == nil else {
            throw unsupported(node, reason: "for-in where clauses are not supported")
        }
        guard let pattern = node.pattern.as(IdentifierPatternSyntax.self) else {
            throw unsupported(node, reason: "only identifier for-in patterns are supported")
        }
        let loopVar = pattern.identifier.text
        let oldLocals = locals
        locals.insert(loopVar)
        let loop = SwiftSourceIR.ctor("swift:for", [
            SwiftSourceIR.varTerm(loopVar),
            try expression(node.sequence),
            try statements(node.body.statements),
        ])
        locals = oldLocals
        effects.add(.opaqueLoop(computeJcsCid(loop)))
        return loop
    }

    private func conditionList(_ conditions: ConditionElementListSyntax) throws -> JcsCanonical {
        guard conditions.count == 1, let first = conditions.first else {
            throw unsupportedFromLine(line: lineOfConditions(conditions), reason: "only a single boolean condition is supported")
        }
        switch first.condition {
        case .expression(let expr):
            return try expression(expr)
        default:
            throw unsupportedFromLine(
                line: lineOfConditions(conditions),
                reason: "only plain boolean expression conditions are supported"
            )
        }
    }

    private func expression(_ expr: ExprSyntax) throws -> JcsCanonical {
        if expr.is(ClosureExprSyntax.self) {
            throw unsupported(expr, reason: "unhandled expression kind: ClosureExprSyntax")
        }
        if expr.is(AwaitExprSyntax.self) || expr.is(TryExprSyntax.self) {
            throw unsupported(expr, reason: "async/try expressions are not supported")
        }
        if expr.is(OptionalChainingExprSyntax.self) {
            throw unsupported(expr, reason: "optional chaining is not supported")
        }
        if expr.is(ForceUnwrapExprSyntax.self) {
            effects.add(.panics)
            throw unsupported(expr, reason: "force unwrap is outside the supported Swift source slice")
        }
        if let ifExpr = expr.as(IfExprSyntax.self) {
            return try ifTerm(ifExpr)
        }

        if let literal = expr.as(IntegerLiteralExprSyntax.self) {
            let raw = literal.literal.text.replacingOccurrences(of: "_", with: "")
            guard let value = Int64(raw) else {
                throw unsupported(expr, reason: "integer literal is out of Int64 range: \(literal.literal.text)")
            }
            return SwiftSourceIR.intConst(value)
        }
        if let literal = expr.as(BooleanLiteralExprSyntax.self) {
            return SwiftSourceIR.boolConst(literal.literal.text == "true")
        }
        if expr.is(NilLiteralExprSyntax.self) {
            return SwiftSourceIR.nilConst()
        }
        if let literal = expr.as(StringLiteralExprSyntax.self) {
            return try stringLiteral(literal)
        }
        if let ref = expr.as(DeclReferenceExprSyntax.self) {
            let name = ref.baseName.text
            if globalVars.contains(name) && !locals.contains(name) {
                effects.add(.reads(name))
            }
            return SwiftSourceIR.varTerm(name)
        }
        if let sequence = expr.as(SequenceExprSyntax.self) {
            return try sequenceExpr(sequence)
        }
        if let prefix = expr.as(PrefixOperatorExprSyntax.self) {
            let op = prefix.operator.text.trimmingCharacters(in: .whitespacesAndNewlines)
            let operand = try expression(prefix.expression)
            switch op {
            case "!": return SwiftSourceIR.ctor("swift:not", [operand])
            case "-": return SwiftSourceIR.ctor("swift:neg", [operand])
            case "+": return SwiftSourceIR.ctor("swift:pos", [operand])
            default:
                throw unsupported(prefix, reason: "unsupported prefix operator: \(op)")
            }
        }
        if let ternary = expr.as(TernaryExprSyntax.self) {
            return SwiftSourceIR.ctor("swift:ternary", [
                try expression(ternary.condition),
                try expression(ternary.thenExpression),
                try expression(ternary.elseExpression),
            ])
        }
        if let member = expr.as(MemberAccessExprSyntax.self) {
            return try memberAccess(member)
        }
        if let call = expr.as(FunctionCallExprSyntax.self) {
            return try functionCall(call)
        }
        if let subscriptCall = expr.as(SubscriptCallExprSyntax.self) {
            return try subscriptCallExpr(subscriptCall)
        }
        if let paren = expr.as(TupleExprSyntax.self),
           paren.elements.count == 1,
           let first = paren.elements.first {
            return try expression(first.expression)
        }

        throw unsupported(expr, reason: "unhandled expression kind: \(type(of: expr))")
    }

    private func sequenceExpr(_ node: SequenceExprSyntax) throws -> JcsCanonical {
        let elements = Array(node.elements)
        guard elements.count == 3 else {
            throw unsupported(node, reason: "only single binary/assignment expressions are supported")
        }
        let lhs = elements[0]
        let op = elements[1].description.trimmingCharacters(in: .whitespacesAndNewlines)
        let rhs = elements[2]

        if op == "=" {
            let target = try lvalue(lhs)
            let value = try expression(rhs)
            addWriteEffectIfGlobalOrStored(targetSyntax: lhs)
            return SwiftSourceIR.ctor("swift:assign", [target, value])
        }

        let opName: String
        switch op {
        case "+": opName = "swift:add"
        case "-": opName = "swift:sub"
        case "*": opName = "swift:mul"
        case "/": opName = "swift:div"
        case "%": opName = "swift:mod"
        case "==": opName = "swift:eq"
        case "!=": opName = "swift:ne"
        case "<": opName = "swift:lt"
        case "<=": opName = "swift:le"
        case ">": opName = "swift:gt"
        case ">=": opName = "swift:ge"
        case "&&": opName = "swift:and"
        case "||": opName = "swift:or"
        case "??": opName = "swift:nilcoalesce"
        case "&": opName = "swift:bitand"
        case "|": opName = "swift:bitor"
        case "^": opName = "swift:bitxor"
        case "<<": opName = "swift:shl"
        case ">>": opName = "swift:shr"
        default:
            throw unsupported(node, reason: "unsupported binary operator: \(op)")
        }
        return SwiftSourceIR.ctor(opName, [try expression(lhs), try expression(rhs)])
    }

    private func stringLiteral(_ node: StringLiteralExprSyntax) throws -> JcsCanonical {
        let raw = node.description.trimmingCharacters(in: .whitespacesAndNewlines)
        guard raw.hasPrefix("\""), raw.hasSuffix("\""), !raw.contains("\\(") else {
            throw unsupported(node, reason: "only non-interpolated string literals are supported")
        }
        let inner = String(raw.dropFirst().dropLast())
        let decoded = inner
            .replacingOccurrences(of: "\\n", with: "\n")
            .replacingOccurrences(of: "\\t", with: "\t")
            .replacingOccurrences(of: "\\\"", with: "\"")
            .replacingOccurrences(of: "\\\\", with: "\\")
        return SwiftSourceIR.stringConst(decoded)
    }

    private func memberAccess(_ node: MemberAccessExprSyntax) throws -> JcsCanonical {
        guard let base = node.base else {
            throw unsupported(node, reason: "implicit member access is not supported")
        }
        let baseTerm = try expression(base)
        let field = node.declName.baseName.text
        let target = "\(base.description.trimmingCharacters(in: .whitespacesAndNewlines)).\(field)"
        if staticVars.contains(qualifiedStaticName(target)) {
            effects.add(.reads(qualifiedStaticName(target)))
        }
        return SwiftSourceIR.ctor("swift:member", [baseTerm, SwiftSourceIR.stringConst(field)])
    }

    private func functionCall(_ node: FunctionCallExprSyntax) throws -> JcsCanonical {
        if node.trailingClosure != nil || !node.additionalTrailingClosures.isEmpty {
            throw unsupported(node, reason: "closures and trailing closures are not supported")
        }
        let callee = node.calledExpression.description.trimmingCharacters(in: .whitespacesAndNewlines)
        let loweredArgs = try node.arguments.map { try expression($0.expression) }

        if isIOCall(callee) {
            effects.add(.io)
        } else if isPanicCall(callee) {
            effects.add(.panics)
        } else {
            effects.add(.unresolvedCall(callee))
        }

        return SwiftSourceIR.ctor("swift:call", [SwiftSourceIR.stringConst(callee)] + loweredArgs)
    }

    private func subscriptCallExpr(_ node: SubscriptCallExprSyntax) throws -> JcsCanonical {
        guard node.arguments.count == 1, let first = node.arguments.first else {
            throw unsupported(node, reason: "only single-index subscripts are supported")
        }
        return SwiftSourceIR.ctor("swift:index", [
            try expression(node.calledExpression),
            try expression(first.expression),
        ])
    }

    private func lvalue(_ expr: ExprSyntax) throws -> JcsCanonical {
        if let ref = expr.as(DeclReferenceExprSyntax.self) {
            return SwiftSourceIR.varTerm(ref.baseName.text)
        }
        if let member = expr.as(MemberAccessExprSyntax.self) {
            return try memberAccess(member)
        }
        if let subscriptCall = expr.as(SubscriptCallExprSyntax.self) {
            return try subscriptCallExpr(subscriptCall)
        }
        throw unsupported(expr, reason: "expression is not assignable: \(type(of: expr))")
    }

    private func addWriteEffectIfGlobalOrStored(targetSyntax: ExprSyntax) {
        if let ref = targetSyntax.as(DeclReferenceExprSyntax.self) {
            let name = ref.baseName.text
            if globalVars.contains(name) && !locals.contains(name) {
                effects.add(.writes(name))
            }
            return
        }
        if let member = targetSyntax.as(MemberAccessExprSyntax.self), let base = member.base {
            let baseText = base.description.trimmingCharacters(in: .whitespacesAndNewlines)
            let field = member.declName.baseName.text
            let target = "\(baseText).\(field)"
            if baseText != "self" {
                effects.add(.writes(qualifiedStaticName(target)))
            }
        }
    }

    private func qualifiedStaticName(_ target: String) -> String {
        if staticVars.contains(target) {
            return target
        }
        let moduleTarget = "\(modulePath(from: sourcePath)).\(target)"
        return staticVars.contains(moduleTarget) ? moduleTarget : target
    }

    private func unsupported<T: SyntaxProtocol>(
        _ node: T,
        kind: String = "unhandled-syntax",
        reason: String
    ) -> UnsupportedSwiftSyntax {
        UnsupportedSwiftSyntax(kind: kind, line: node.startLocation(converter: converter).line, reason: reason)
    }

    private func unsupportedFromLine(line: Int?, reason: String) -> UnsupportedSwiftSyntax {
        UnsupportedSwiftSyntax(kind: "unhandled-syntax", line: line, reason: reason)
    }

    private func lineOfConditions(_ conditions: ConditionElementListSyntax) -> Int? {
        conditions.first?.startLocation(converter: converter).line
    }
}

private func modulePath(from path: String) -> String {
    let withoutExtension = (path as NSString).deletingPathExtension
    let components = withoutExtension
        .split(separator: "/")
        .map { sanitizeIdentifierComponent(String($0)) }
        .filter { !$0.isEmpty }
    return components.isEmpty ? "SwiftSource" : components.joined(separator: ".")
}

private func sanitizeIdentifierComponent(_ value: String) -> String {
    let allowed = value.map { character -> Character in
        if character.isLetter || character.isNumber || character == "_" {
            return character
        }
        return "_"
    }
    let result = String(allowed)
    return result.trimmingCharacters(in: CharacterSet(charactersIn: "_"))
}

private func cleanTypeName(_ value: String) -> String {
    value.trimmingCharacters(in: .whitespacesAndNewlines)
        .replacingOccurrences(of: " ", with: "")
}

private func isPath(_ path: String, inside root: String) -> Bool {
    if path == root {
        return true
    }
    let prefix = root.hasSuffix("/") ? root : root + "/"
    return path.hasPrefix(prefix)
}

private func relativePath(_ path: String, root: String) -> String {
    let prefix = root.hasSuffix("/") ? root : root + "/"
    if path.hasPrefix(prefix) {
        return String(path.dropFirst(prefix.count))
    }
    return path
}

private func isIOCall(_ callee: String) -> Bool {
    callee == "print"
        || callee.hasPrefix("FileHandle")
        || callee.hasPrefix("URLSession")
        || callee.contains(".write")
        || callee.contains(".read")
}

private func isPanicCall(_ callee: String) -> Bool {
    callee == "fatalError"
        || callee == "preconditionFailure"
        || callee == "assertionFailure"
        || callee == "precondition"
}
