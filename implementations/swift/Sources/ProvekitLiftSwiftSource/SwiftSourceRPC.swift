// SPDX-License-Identifier: Apache-2.0

import Foundation
import ProvekitCrypto

public enum SwiftSourceRPC {
    public static let surface = "swift-source"
    public static let version = "0.1.0-draft"

    public static func initializeResult() -> [String: Any] {
        [
            "name": "provekit-lift-swift-source",
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

    public static func run() {
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
                    response = errorResponse(id: nil, code: -32700, message: "PARSE_ERROR")
                    write(response)
                    continue
                }
                response = try dispatch(request)
            } catch let error as SwiftSourceRPCExit {
                write(error.response)
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
            return try lift(id: id, params: params)
        case "compile":
            return try compile(id: id, params: params)
        case "shutdown":
            throw SwiftSourceRPCExit(response: response(id: id, result: NSNull()))
        case "exit":
            throw SwiftSourceRPCExit(response: response(id: id, result: NSNull()))
        default:
            return errorResponse(id: id, code: -32601, message: "METHOD_NOT_FOUND: \(method)")
        }
    }

    private static func lift(id: Any, params: [String: Any]) throws -> [String: Any] {
        let requestedSurface = params["surface"] as? String ?? surface
        guard requestedSurface == surface else {
            return errorResponse(id: id, code: 1003, message: "SURFACE_NOT_SUPPORTED: \(requestedSurface)")
        }

        guard let sourcePathsAny = params["source_paths"] as? [Any], !sourcePathsAny.isEmpty else {
            return errorResponse(id: id, code: -32602, message: "source_paths must be a non-empty array")
        }
        let sourcePaths = sourcePathsAny.compactMap { $0 as? String }.filter { !$0.isEmpty }
        guard !sourcePaths.isEmpty else {
            return errorResponse(id: id, code: -32602, message: "source_paths must contain strings")
        }
        let workspaceRoot = params["workspace_root"] as? String ?? "."
        let result = SwiftSourceLifter.liftPaths(workspaceRoot: workspaceRoot, sourcePaths: sourcePaths)

        return response(id: id, result: [
            "kind": "ir-document",
            "ir": result.ir.map(SwiftSourceIR.anyValue),
            "callEdges": [],
            "diagnostics": result.diagnostics.map(SwiftSourceIR.anyValue),
            "opacityReport": result.opacityReport.map(SwiftSourceIR.anyValue),
            "refusals": result.refusals.map(SwiftSourceIR.anyValue),
        ])
    }

    private static func compile(id: Any, params: [String: Any]) throws -> [String: Any] {
        guard let irAny = params["ir"] else {
            return errorResponse(id: id, code: -32602, message: "ir required")
        }

        let body: String
        if let array = irAny as? [Any] {
            let ir = try array.map(SwiftSourceIR.canonical(from:))
            body = try SwiftSourceCompiler.compileIRDocument(ir)
        } else if let object = irAny as? [String: Any] {
            body = try SwiftSourceCompiler.compileBodyTerm(try SwiftSourceIR.canonical(from: object))
        } else {
            return errorResponse(id: id, code: -32602, message: "ir must be an array or object")
        }

        return response(id: id, result: [
            "kind": "compiled-formula",
            "body": body,
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
              let data = try? JSONSerialization.data(withJSONObject: object, options: [.sortedKeys]),
              let line = String(data: data, encoding: .utf8)
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

private struct SwiftSourceRPCExit: Error {
    let response: [String: Any]
}
