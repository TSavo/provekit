namespace Provekit.LSP;

/// <summary>
/// ForwardPropagator: accumulate posts and emit implication-check diagnostics.
/// Per: docs/lsp/forward-propagation-floor-v1.md
/// </summary>
public class ForwardPropagator
{
    private Dictionary<string, Post> _seedCatalog = new();

    public class Post
    {
        public List<string> Constraints { get; }
        public bool IsTop { get; }

        public Post(List<string> constraints, bool isTop)
        {
            Constraints = constraints;
            IsTop = isTop;
        }

        public static Post Top() => new(new List<string>(), true);
        public static Post Of(string constraint) => new(new List<string> { constraint }, false);
    }

    public class DiagnosticResult
    {
        public string Code { get; }
        public string Message { get; }

        public DiagnosticResult(string code, string message)
        {
            Code = code;
            Message = message;
        }
    }

    public void AddToCatalog(string calleeId, Post pre, Post post)
    {
        _seedCatalog[calleeId] = post;
    }

    public DiagnosticResult? CheckCallsite(string calleeId, Post currentPost)
    {
        if (currentPost.IsTop) return null;
        if (!_seedCatalog.TryGetValue(calleeId, out var calleePre)) return null;

        foreach (var c in currentPost.Constraints)
        {
            if (!calleePre.Constraints.Contains(c))
            {
                return new DiagnosticResult("provekit.lsp.implication_failed",
                    $"post does not imply callee pre: {string.Join(" && ", calleePre.Constraints)}");
            }
        }
        return null;
    }
}
