// SPDX-License-Identifier: Apache-2.0

import Foundation
import ProvekitCrypto
import ProvekitLiftSwiftSource

private struct TestFailure: Error, CustomStringConvertible {
    let message: String
    var description: String { message }
}

@main
struct SwiftSourceLiftTests {
    static func main() {
        let tests: [(String, () throws -> Void)] = [
            ("sugar body emits ast_template alongside body_text", testSugarBodyEmitsAstTemplateAlongsideBodyText),
            ("sugar template cid is stable under parameter renaming", testSugarBodyTemplateCidIsStableUnderParameterRenaming),
            ("recognize emits exact tag", testRecognizeEmitsExactTag),
            ("recognize returns no tags for non-match", testRecognizeReturnsNoTagsForNonMatch),
            ("recognize routes multiple bindings", testRecognizeRoutesMultipleBindings),
            ("recognize paths self-resolves sugar templates", testRecognizePathsSelfResolvesSugarTemplates),
            ("recognize RPC self-resolves sugar templates", testRecognizeRPCSelfResolvesSugarTemplates),
            ("recognize RPC returns no tags for non-match", testRecognizeRPCReturnsNoTagsForNonMatch),
        ]

        var failures: [String] = []
        for (name, test) in tests {
            do {
                try test()
                print("ok - \(name)")
            } catch {
                failures.append("\(name): \(error)")
                print("not ok - \(name): \(error)", to: .standardError)
            }
        }

        if !failures.isEmpty {
            print("\nSwift source lift tests failed:", to: .standardError)
            for failure in failures {
                print("  - \(failure)", to: .standardError)
            }
            exit(1)
        }
    }
}

private func testSugarBodyEmitsAstTemplateAlongsideBodyText() throws {
    let source = """
    @ProveKitSugar(concept: "concept:http-request", library: "urlsession")
    func fetchStatus(_ url: String, headers: String) -> Int {
        let request = makeRequest(url, headers)
        return send(request)
    }
    """

    let entries = SwiftSourceLifter.liftLibraryBindingsSource(source, path: "Sources/Shim.swift").ir
    try expect(entries.count == 1, "expected one sugar binding entry, got \(entries.count)")
    let entry = try object(entries[0])
    try expect(entry["kind"] as? String == "library-sugar-binding-entry", "wrong entry kind")
    try expect(entry["target_language"] as? String == "swift", "wrong target language")
    try expect(entry["target_library_tag"] as? String == "urlsession", "wrong library tag")
    try expect(entry["concept_name"] as? String == "concept:http-request", "wrong concept")
    try expect(entry["source_function_name"] as? String == "fetchStatus", "wrong source function")
    try expect(entry["param_names"] as? [String] == ["url", "headers"], "wrong param names")

    let body = try require(entry["body_source"] as? [String: Any], "missing body_source")
    try expect((body["body_text"] as? String ?? "").contains("makeRequest(url, headers)"), "missing body_text")
    try expect(body["ast_template"] != nil, "missing ast_template")
    try expect((body["template_cid"] as? String ?? "").hasPrefix("blake3-512:"), "missing template_cid")
    try expect(body["param_names"] as? [String] == ["url", "headers"], "wrong body param_names")
}

private func testSugarBodyTemplateCidIsStableUnderParameterRenaming() throws {
    let sourceA = """
    @ProveKitSugar(concept: "concept:http-request", library: "urlsession")
    func fetchStatus(_ url: String) -> Int {
        return send(url)
    }
    """
    let sourceB = """
    @ProveKitSugar(concept: "concept:http-request", library: "urlsession")
    func fetchStatus(_ endpoint: String) -> Int {
        return send(endpoint)
    }
    """

    let bodyA = try sugarBodySource(sourceA)
    let bodyB = try sugarBodySource(sourceB)
    try expect(bodyA["template_cid"] as? String == bodyB["template_cid"] as? String, "template CID changed under parameter rename")
    try expect(
        try stableJSONString(bodyA["ast_template"] as Any) == stableJSONString(bodyB["ast_template"] as Any),
        "AST template changed under parameter rename"
    )
    try expect(bodyA["body_text"] as? String != bodyB["body_text"] as? String, "body_text should preserve source spelling")
}

private func testRecognizeEmitsExactTag() throws {
    let binding = try bindingTemplate(from: """
    @ProveKitSugar(concept: "concept:http-request", library: "urlsession")
    func fetchStatus(_ url: String) -> Int {
        return send(url)
    }
    """)

    let tags = SwiftSourceLifter.recognizeSource(
        """
        func getStatus(_ endpoint: String) -> Int {
            return send(endpoint)
        }
        """,
        path: "Sources/Client.swift",
        bindingTemplates: [binding]
    )

    try expect(tags.count == 1, "expected one tag, got \(tags.count)")
    let tag = try object(tags[0])
    try expect(tag["file"] as? String == "Sources/Client.swift", "wrong file")
    try expect(tag["function_name"] as? String == "getStatus", "wrong recognized function")
    try expect(tag["concept_name"] as? String == "concept:http-request", "wrong concept")
    try expect(tag["library_tag"] as? String == "urlsession", "wrong library")
    try expect(tag["template_cid"] as? String == stringField("template_cid", in: binding), "wrong template CID")
    try expect(tag["match_tier"] as? String == "exact", "wrong match tier")
    let paramBindings = try require(tag["param_bindings"] as? [[String: Any]], "missing param bindings")
    try expect(paramBindings.count == 1, "wrong param binding count")
    try expect(paramBindings[0]["index"] as? Int == 1, "wrong binding index")
    try expect(paramBindings[0]["source_text"] as? String == "endpoint", "wrong binding source text")
}

private func testRecognizeReturnsNoTagsForNonMatch() throws {
    let binding = try bindingTemplate(from: """
    @ProveKitSugar(concept: "concept:http-request", library: "urlsession")
    func fetchStatus(_ url: String) -> Int {
        return send(url)
    }
    """)

    let tags = SwiftSourceLifter.recognizeSource(
        """
        func getStatus(_ endpoint: String) -> Int {
            return 200
        }
        """,
        path: "Sources/Client.swift",
        bindingTemplates: [binding]
    )

    try expect(tags.isEmpty, "expected no tags, got \(tags.count)")
}

private func testRecognizeRoutesMultipleBindings() throws {
    let httpBinding = try bindingTemplate(from: """
    @ProveKitSugar(concept: "concept:http-request", library: "urlsession")
    func fetchStatus(_ url: String) -> Int {
        return send(url)
    }
    """)
    let sqlBinding = try bindingTemplate(from: """
    @ProveKitSugar(concept: "concept:sql-execute", library: "sqlite")
    func execute(_ db: String, sql: String) -> Int {
        return run(db, sql)
    }
    """)

    let tags = SwiftSourceLifter.recognizeSource(
        """
        func getStatus(_ endpoint: String) -> Int {
            return send(endpoint)
        }

        func save(_ conn: String, statement: String) -> Int {
            return run(conn, statement)
        }
        """,
        path: "Sources/Client.swift",
        bindingTemplates: [httpBinding, sqlBinding]
    )

    let concepts = Set(try tags.map { try require(object($0)["concept_name"] as? String, "missing concept") })
    try expect(concepts == ["concept:http-request", "concept:sql-execute"], "wrong concepts: \(concepts)")
}

private func testRecognizePathsSelfResolvesSugarTemplates() throws {
    let root = try FileManager.default.url(
        for: .itemReplacementDirectory,
        in: .userDomainMask,
        appropriateFor: URL(fileURLWithPath: NSTemporaryDirectory()),
        create: true
    )
    defer { try? FileManager.default.removeItem(at: root) }
    try """
    @ProveKitSugar(concept: "concept:http-request", library: "urlsession", family: "concept:family:http")
    func fetch(_ url: String) -> Int {
        return send(url)
    }
    """.write(to: root.appendingPathComponent("Shim.swift"), atomically: true, encoding: .utf8)
    try """
    func getStatus(_ endpoint: String) -> Int {
        return send(endpoint)
    }
    """.write(to: root.appendingPathComponent("Client.swift"), atomically: true, encoding: .utf8)

    let tags = SwiftSourceLifter.recognizePaths(
        workspaceRoot: root.path,
        sourcePaths: ["Shim.swift", "Client.swift"],
        bindingTemplates: []
    )

    try expect(tags.count == 1, "expected one self-resolved tag, got \(tags.count)")
    let tag = try object(tags[0])
    try expect(tag["file"] as? String == "Client.swift", "wrong file: \(tag)")
    try expect(tag["function_name"] as? String == "getStatus", "wrong function: \(tag)")
    try expect(tag["concept_name"] as? String == "concept:http-request", "wrong concept: \(tag)")
    try expect(tag["library_tag"] as? String == "urlsession", "wrong library: \(tag)")
}

private func testRecognizeRPCSelfResolvesSugarTemplates() throws {
    let root = try temporarySwiftRecognitionProject(
        clientSource: """
        func getStatus(_ endpoint: String) -> Int {
            return send(endpoint)
        }
        """
    )
    defer { try? FileManager.default.removeItem(at: root) }

    let response = SwiftSourceRPC.handle([
        "jsonrpc": "2.0",
        "id": 1,
        "method": "provekit.plugin.recognize",
        "params": [
            "project_root": root.path,
            "source_paths": ["Shim.swift", "Client.swift"],
        ],
    ])
    let tags = try rpcTags(response)

    try expect(tags.count == 1, "expected one RPC tag, got \(tags.count)")
    let tag = try require(tags.first, "missing RPC tag")
    try expect(tag["file"] as? String == "Client.swift", "wrong RPC file: \(tag)")
    try expect(tag["function_name"] as? String == "getStatus", "wrong RPC function: \(tag)")
    try expect(tag["concept_name"] as? String == "concept:http-request", "wrong RPC concept: \(tag)")
    try expect(tag["library_tag"] as? String == "urlsession", "wrong RPC library: \(tag)")
}

private func testRecognizeRPCReturnsNoTagsForNonMatch() throws {
    let root = try temporarySwiftRecognitionProject(
        clientSource: """
        func getStatus(_ endpoint: String) -> Int {
            return 200
        }
        """
    )
    defer { try? FileManager.default.removeItem(at: root) }

    let response = SwiftSourceRPC.handle([
        "jsonrpc": "2.0",
        "id": 1,
        "method": "provekit.plugin.recognize",
        "params": [
            "project_root": root.path,
            "source_paths": ["Shim.swift", "Client.swift"],
        ],
    ])
    let tags = try rpcTags(response)

    try expect(tags.isEmpty, "expected no RPC tags, got \(tags.count)")
}

private func temporarySwiftRecognitionProject(clientSource: String) throws -> URL {
    let root = try FileManager.default.url(
        for: .itemReplacementDirectory,
        in: .userDomainMask,
        appropriateFor: URL(fileURLWithPath: NSTemporaryDirectory()),
        create: true
    )
    try """
    @ProveKitSugar(concept: "concept:http-request", library: "urlsession", family: "concept:family:http")
    func fetch(_ url: String) -> Int {
        return send(url)
    }
    """.write(to: root.appendingPathComponent("Shim.swift"), atomically: true, encoding: .utf8)
    try clientSource.write(to: root.appendingPathComponent("Client.swift"), atomically: true, encoding: .utf8)
    return root
}

private func rpcTags(_ response: [String: Any]) throws -> [[String: Any]] {
    if let error = response["error"] {
        throw TestFailure(message: "unexpected RPC error: \(error)")
    }
    let result = try require(response["result"] as? [String: Any], "missing RPC result")
    return try require(result["tags"] as? [[String: Any]], "missing RPC tags")
}

private func sugarBodySource(_ source: String) throws -> [String: Any] {
    let entries = SwiftSourceLifter.liftLibraryBindingsSource(source, path: "Sources/Shim.swift").ir
    try expect(entries.count == 1, "expected one entry, got \(entries.count)")
    let entry = try object(entries[0])
    return try require(entry["body_source"] as? [String: Any], "missing body_source")
}

private func bindingTemplate(from source: String) throws -> JcsCanonical {
    let entries = SwiftSourceLifter.liftLibraryBindingsSource(source, path: "Sources/Shim.swift").ir
    try expect(entries.count == 1, "expected one entry, got \(entries.count)")
    let entry = try object(entries[0])
    let body = try require(entry["body_source"] as? [String: Any], "missing body_source")
    let paramNames = (body["param_names"] as? [String] ?? []).map { JcsCanonical.string($0) }
    return .object([
        ("concept_name", .string(try require(entry["concept_name"] as? String, "missing concept"))),
        ("library_tag", .string(try require(entry["target_library_tag"] as? String, "missing library"))),
        ("family", stringOrNull(entry["family"])),
        ("ast_template", try jcsCanonical(from: try require(body["ast_template"], "missing ast_template"))),
        ("template_cid", .string(try require(body["template_cid"] as? String, "missing template_cid"))),
        ("param_names", .array(paramNames)),
        ("contract_cid", stringOrNull(entry["contract_cid"])),
    ])
}

private func object(_ value: JcsCanonical) throws -> [String: Any] {
    let data = JcsCanonicalizer.encode(value)
    return try require(JSONSerialization.jsonObject(with: data) as? [String: Any], "not a JSON object")
}

private func stableJSONString(_ value: Any) throws -> String {
    JcsCanonicalizer.encodeString(try jcsCanonical(from: value))
}

private func jcsCanonical(from value: Any) throws -> JcsCanonical {
    if value is NSNull {
        return .null
    }
    if let value = value as? Bool {
        return .bool(value)
    }
    if let value = value as? Int {
        return .int(Int64(value))
    }
    if let value = value as? Int64 {
        return .int(value)
    }
    if let value = value as? String {
        return .string(value)
    }
    if let value = value as? [Any] {
        return .array(try value.map(jcsCanonical(from:)))
    }
    if let value = value as? [String: Any] {
        return .object(try value.map { key, child in
            (key, try jcsCanonical(from: child))
        })
    }
    throw TestFailure(message: "unsupported JSON value: \(value)")
}

private func stringOrNull(_ value: Any?) -> JcsCanonical {
    if let value = value as? String {
        return .string(value)
    }
    return .null
}

private func stringField(_ name: String, in value: JcsCanonical) -> String? {
    guard case .object(let pairs) = value,
          let field = pairs.first(where: { $0.0 == name })?.1,
          case .string(let string) = field
    else {
        return nil
    }
    return string
}

private func expect(_ condition: Bool, _ message: String) throws {
    if !condition {
        throw TestFailure(message: message)
    }
}

private func require<T>(_ value: T?, _ message: String) throws -> T {
    guard let value else {
        throw TestFailure(message: message)
    }
    return value
}

private func print(_ value: String, to handle: FileHandle) {
    handle.write((value + "\n").data(using: .utf8)!)
}
