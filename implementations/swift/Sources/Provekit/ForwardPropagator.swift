import Foundation

/// ForwardPropagator: accumulate posts and emit implication-check diagnostics.
/// Per: docs/lsp/forward-propagation-floor-v1.md
public class ForwardPropagator {
    private var seedCatalog: [String: Post] = [:]

    public class Post {
        public let constraints: [String]
        public let isTop: Bool

        public init(constraints: [String], isTop: Bool) {
            self.constraints = constraints
            self.isTop = isTop
        }

        public static func top() -> Post {
            return Post(constraints: [], isTop: true)
        }

        public static func of(_ constraint: String) -> Post {
            return Post(constraints: [constraint], isTop: false)
        }
    }

    public class DiagnosticResult {
        public let code: String
        public let message: String

        public init(code: String, message: String) {
            self.code = code
            self.message = message
        }
    }

    public func addToCatalog(_ calleeId: String, pre: Post, post: Post) {
        seedCatalog[calleeId] = post
    }

    public func checkCallsite(_ calleeId: String, currentPost: Post) -> DiagnosticResult? {
        if currentPost.isTop { return nil }
        guard let calleePre = seedCatalog[calleeId] else { return nil }
        for constraint in currentPost.constraints {
            if !calleePre.constraints.contains(constraint) {
                return DiagnosticResult(
                    code: "provekit.lsp.implication_failed",
                    message: "post does not imply callee pre: \(calleePre.constraints.joined(separator: " && "))"
                )
            }
        }
        return nil
    }
}
