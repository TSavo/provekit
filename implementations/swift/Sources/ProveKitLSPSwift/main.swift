// SPDX-License-Identifier: Apache-2.0
//
// provekit-lsp-swift — NDJSON LSP plugin for Swift.
//
// Protocol (parse-protocol v1):
//   {"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
//   {"jsonrpc":"2.0","id":2,"method":"parse","params":{"path":"...","source":"..."}}
//   {"jsonrpc":"2.0","id":3,"method":"shutdown"}
//
// Reads NDJSON from stdin, writes NDJSON to stdout.
// Each line is one JSON-RPC message.
//
// Corresponds to the Go LSP plugin at
// implementations/go/cmd/provekit-lsp-go/main.go — same wire shape.
//
// Regex-based lifter (v0). SwiftSyntax-based AST lifting is future work.

import Foundation
import SwiftLifter

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
        "capabilities": ["parse"],
        "name": "provekit-lsp-swift",
        "version": "0.1.0",
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
