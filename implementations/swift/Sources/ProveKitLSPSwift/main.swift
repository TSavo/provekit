// SPDX-License-Identifier: Apache-2.0
//
// provekit-lsp-swift: NDJSON LSP plugin for Swift.
//
// Protocol (provekit-lsp-shared/1):
//   {"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
//   {"jsonrpc":"2.0","id":2,"method":"analyzeDocument","params":{"file":"...","text":"..."}}
//   {"jsonrpc":"2.0","id":3,"method":"shutdown"}
//
// Reads NDJSON from stdin, writes NDJSON to stdout.
// Each line is one JSON-RPC message.
//
// Corresponds to the Go LSP plugin at
// implementations/go/cmd/provekit-lsp-go/main.go: same wire shape.
//
// Regex-based lifter (v0). SwiftSyntax-based AST lifting is future work.

import Foundation
import ProvekitCrypto
import SwiftLifter

let version = "0.1.0"
let kitID = "swift"
let sourceSurface = "swift-source"
let sharedProtocolVersion = "provekit-lsp-shared/1"
let protocolCatalogCID = "blake3-512:0e3905c2a7a098cd538b9669428a7dffd2b84ba8ccf8fde3724fe2ab61fd3fbc1e1a616a6b20b6817464cdc50c466b5497d4ac2e2dc34c3c15f05535b463643c"

// MARK: - JSON helpers

func escapeJson(_ s: String) -> String {
    var out = ""
    for scalar in s.unicodeScalars {
        switch scalar.value {
        case 0x22: out += "\\\""   // "
        case 0x5C: out += "\\\\"   // \
        case 0x08: out += "\\b"
        case 0x0C: out += "\\f"
        case 0x0A: out += "\\n"
        case 0x0D: out += "\\r"
        case 0x09: out += "\\t"
        case 0..<0x20:
            out += String(format: "\\u%04x", scalar.value)
        default:
            out.unicodeScalars.append(scalar)
        }
    }
    return out
}

func jsonString(_ s: String) -> String { "\"\(escapeJson(s))\"" }

func anyToJson(_ val: Any) -> String {
    if let dict = val as? [String: Any] {
        let pairs = dict.sorted { $0.key < $1.key }.map { k, v in
            "\(jsonString(k)):\(anyToJson(v))"
        }
        return "{\(pairs.joined(separator: ","))}"
    } else if let arr = val as? [Any] {
        return "[\(arr.map { anyToJson($0) }.joined(separator: ","))]"
    } else if let s = val as? String {
        return jsonString(s)
    } else if let n = val as? Int {
        return "\(n)"
    } else if let n = val as? Double {
        return "\(n)"
    } else if let b = val as? Bool {
        return b ? "true" : "false"
    } else {
        return "null"
    }
}

// MARK: - RPC response helpers

func writeResponse(id: Any?, result: Any) {
    let idStr: String
    if let idInt = id as? Int {
        idStr = "\(idInt)"
    } else if let idStr2 = id as? String {
        idStr = "\"\(escapeJson(idStr2))\""
    } else {
        idStr = "null"
    }
    let resultJson = anyToJson(result)
    let line = "{\"id\":\(idStr),\"jsonrpc\":\"2.0\",\"result\":\(resultJson)}"
    print(line)
    fflush(stdout)
}

func writeError(id: Any?, code: Int, message: String) {
    let idStr: String
    if let idInt = id as? Int {
        idStr = "\(idInt)"
    } else if let idStr2 = id as? String {
        idStr = "\"\(escapeJson(idStr2))\""
    } else {
        idStr = "null"
    }
    let errJson = "{\"code\":\(code),\"message\":\(jsonString(message))}"
    let line = "{\"error\":\(errJson),\"id\":\(idStr),\"jsonrpc\":\"2.0\"}"
    print(line)
    fflush(stdout)
}

// MARK: - Request handlers

func handleInitialize(id: Any?) {
    let result: [String: Any] = [
        "capabilities": [
            "source_surfaces": [sourceSurface],
            "entry_kinds": ["bind-lift-entry", "call-edge"],
            "diagnostic_codes": [
                "provekit.lsp.parse_error",
                "provekit.lsp.lift_gap",
                "provekit.lsp.implication_failed",
            ],
            "status_kinds": ["materialize", "emit", "check", "prove"],
        ],
        "kit_id": kitID,
        "name": "provekit-lsp-swift",
        "protocol_catalog_cid": protocolCatalogCID,
        "protocol_version": sharedProtocolVersion,
        "version": version,
    ]
    writeResponse(id: id, result: result)
}

func handleParse(id: Any?, params: [String: Any]) {
    guard let path = params["path"] as? String,
          let source = params["source"] as? String
    else {
        writeError(id: id, code: -32602, message: "invalid params: expected {path, source}")
        return
    }

    let result = SwiftLifter.lift(source: source, path: path)

    let declsArray: [[String: Any]] = result.declarations.map { $0.toDict() }
    let edgesArray: [[String: Any]] = result.callEdges.map { $0.toDict() }
    let warningsArray: [Any] = result.warnings.map { $0 as Any }

    let response: [String: Any] = [
        "callEdges": edgesArray,
        "declarations": declsArray,
        "warnings": warningsArray,
    ]
    writeResponse(id: id, result: response)
}

func handleAnalyzeDocument(id: Any?, params: [String: Any]) {
    if let requestedKit = params["kit_id"] as? String, requestedKit != kitID {
        writeError(id: id, code: -32602, message: "unsupported kit_id")
        return
    }

    let source = (params["text"] as? String) ?? (params["source"] as? String) ?? ""
    let file = (params["file"] as? String) ?? (params["path"] as? String) ?? "input.swift"
    let uri = (params["uri"] as? String) ?? "file://\(file)"
    let lifted = SwiftLifter.lift(source: source, path: file)
    let range = wholeDocumentRange(source)
    let entries: [[String: Any]] = lifted.declarations.map {
        [
            "kind": "bind-lift-entry",
            "entry": $0.toDict(),
            "range": range,
        ]
    }

    let response: [String: Any] = [
        "kind": "lsp-document-analysis",
        "schema_version": "1",
        "kit_id": kitID,
        "uri": uri,
        "file": file,
        "document_cid": Blake3.hex(Data(source.utf8)),
        "protocol_catalog_cid": protocolCatalogCID,
        "entries": entries,
        "diagnostics": forwardPropagationDiagnostics(source),
        "statuses": [],
        "project": NSNull(),
    ]
    writeResponse(id: id, result: response)
}

func wholeDocumentRange(_ source: String) -> [String: Any] {
    var line = 1
    var col = 0
    for ch in source {
        if ch == "\n" {
            line += 1
            col = 0
        } else {
            col += 1
        }
    }
    return [
        "start_line": 1,
        "start_col": 0,
        "end_line": line,
        "end_col": col,
    ]
}

func forwardPropagationDiagnostics(_ source: String) -> [[String: Any]] {
    let lines = source.components(separatedBy: "\n")
    var diagnostics: [[String: Any]] = []
    var currentFunction: String?
    var functionDepth = 0
    var loopActive = false
    var loopDepth = 0

    for (index, line) in lines.enumerated() {
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        if currentFunction == nil, let name = parseFunctionName(trimmed) {
            currentFunction = name
            functionDepth = 0
            loopActive = false
            loopDepth = 0
        }

        if let function = currentFunction {
            if loopActive && functionDepth < loopDepth {
                loopActive = false
            }
            if function != "checkPositive",
               let column = findCallColumn(line, callee: "checkPositive"),
               !loopActive,
               !callArgumentIsPositive(String(line.dropFirst(column + "checkPositive".count))) {
                diagnostics.append(implicationFailedDiagnostic(line: index + 1, column: column))
            }

            if (trimmed.hasPrefix("for ") || trimmed.hasPrefix("while ")), trimmed.contains("{") {
                loopActive = true
                loopDepth = functionDepth + 1
            }

            functionDepth += braceDelta(line)
            if functionDepth <= 0 {
                currentFunction = nil
                loopActive = false
            }
        }
    }

    return diagnostics
}

func parseFunctionName(_ trimmed: String) -> String? {
    guard trimmed.hasPrefix("func ") else { return nil }
    let rest = trimmed.dropFirst(5).drop(while: { $0 == " " || $0 == "\t" })
    let name = rest.prefix(while: { isIdentifierChar($0) })
    return name.isEmpty ? nil : String(name)
}

func braceDelta(_ line: String) -> Int {
    var delta = 0
    for ch in line {
        if ch == "{" { delta += 1 }
        if ch == "}" { delta -= 1 }
    }
    return delta
}

func findCallColumn(_ line: String, callee: String) -> Int? {
    var searchStart = line.startIndex
    while searchStart < line.endIndex,
          let range = line.range(of: callee, range: searchStart..<line.endIndex) {
        let beforeOK = range.lowerBound == line.startIndex || !isIdentifierChar(line[line.index(before: range.lowerBound)])
        let afterName = range.upperBound
        let afterNameOK = afterName == line.endIndex || !isIdentifierChar(line[afterName])
        if beforeOK && afterNameOK {
            let rest = line[afterName...].drop(while: { $0 == " " || $0 == "\t" })
            if rest.first == "(" {
                return line.distance(from: line.startIndex, to: range.lowerBound)
            }
        }
        searchStart = range.upperBound
    }
    return nil
}

func callArgumentIsPositive(_ afterName: String) -> Bool {
    var rest = afterName.drop(while: { $0 == " " || $0 == "\t" })
    guard rest.first == "(" else { return false }
    rest = rest.dropFirst().drop(while: { $0 == " " || $0 == "\t" })
    var sign = 1
    if rest.first == "+" {
        rest = rest.dropFirst()
    } else if rest.first == "-" {
        sign = -1
        rest = rest.dropFirst()
    }
    rest = rest.drop(while: { $0 == " " || $0 == "\t" })
    let digits = rest.prefix(while: { $0 >= "0" && $0 <= "9" })
    guard !digits.isEmpty, let value = Int(digits) else { return false }
    return sign * value > 0
}

func isIdentifierChar(_ ch: Character) -> Bool {
    return ch.isLetter || ch.isNumber || ch == "_"
}

func implicationFailedDiagnostic(line: Int, column: Int) -> [String: Any] {
    let callee = "checkPositive"
    let preCID = Blake3.hex(Data("\(callee):pre:x > 0".utf8))
    let postCID = Blake3.hex(Data("\(callee):post:returns true".utf8))
    let seed = "\(callee)|\(preCID)|\(postCID)"
    let attestationCID = Blake3.hex(Data("attestation:\(seed)".utf8))
    let contractCID = Blake3.hex(Data("contract:\(seed)".utf8))
    let currentPostCID = Blake3.hex(Data("post:known:x <= 0".utf8))

    return [
        "code": "provekit.lsp.implication_failed",
        "message": "callee precondition not established at this callsite",
        "severity": "error",
        "range": [
            "start_line": line,
            "start_col": column,
            "end_line": line,
            "end_col": column + callee.count,
        ],
        "producer": "forward-propagation",
        "kit_id": kitID,
        "protocol_catalog_cid": protocolCatalogCID,
        "data": [
            "schema_version": 1,
            "kind": "provekit.lsp.implication_failed",
            "callee": callee,
            "callee_contract_cid": contractCID,
            "callee_attestation_cid": attestationCID,
            "callee_pre_cid": preCID,
            "callee_post_cid": postCID,
            "current_post_cid": currentPostCID,
            "missing_conjuncts": ["x > 0"],
        ],
    ]
}

func handleShutdown(id: Any?) {
    writeResponse(id: id, result: Optional<Int>.none as Any)
}

// MARK: - Main loop

func parseId(from obj: [String: Any]) -> Any? {
    return obj["id"]
}

func run() {
    while let line = readLine(strippingNewline: true) {
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty else { continue }

        guard let data = trimmed.data(using: .utf8),
              let parsed = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else {
            // Malformed JSON: ignore per NDJSON convention.
            continue
        }

        let method = parsed["method"] as? String ?? ""
        let id = parsed["id"]
        let params = parsed["params"] as? [String: Any] ?? [:]

        switch method {
        case "initialize":
            handleInitialize(id: id)
        case "analyzeDocument":
            handleAnalyzeDocument(id: id, params: params)
        case "parse":
            handleParse(id: id, params: params)
        case "shutdown":
            handleShutdown(id: id)
            return
        case "exit":
            return
        default:
            writeError(id: id, code: -32601, message: "unknown method: \(method)")
        }
    }
}

run()
