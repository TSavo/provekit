using System.Text.Json;

namespace Provekit.Verifier;

/// <summary>
/// MementoPool: the verification state machine.
/// Architecture principle: the memento IS the verification.
/// To verify something is to find its memento in the pool.
/// The .proof protocol IS the cache.
/// The hash IS the boundary.
/// </summary>
public class MementoPool
{
    /// <summary>CID → parsed envelope. The memento IS the verification.</summary>
    public Dictionary<string, JsonElement> Mementos { get; } = new();

    /// <summary>Formula CID → memento CID. Index for fast formula lookup.</summary>
    public Dictionary<string, string> FormulaToMemento { get; } = new();

    /// <summary>sourceSymbol → bridge envelope.</summary>
    public Dictionary<string, JsonElement> BridgesBySymbol { get; } = new();

    public List<LoadError> LoadErrors { get; } = new();

    /// <summary>
    /// Look up a formula by its content hash.
    /// The memento IS the verification; if found, the formula is verified.
    /// No solver is invoked.
    /// </summary>
    public JsonElement? VerifyByHash(string formulaCid)
    {
        if (!FormulaToMemento.TryGetValue(formulaCid, out var mementoCid))
            return null;
        if (!Mementos.TryGetValue(mementoCid, out var memento))
            return null;
        return memento;
    }

    /// <summary>
    /// Insert a memento into the pool and index it by formula hash.
    /// The .proof protocol IS the cache: storing a memento IS caching
    /// the verification result.
    /// </summary>
    public void Insert(string mementoCid, JsonElement envelope)
    {
        // Index by formula hashes in evidence body
        if (envelope.TryGetProperty("evidence", out var evidence))
        {
            if (evidence.TryGetProperty("body", out var body))
            {
                foreach (var field in new[] { "preHash", "postHash", "invHash", "antecedentHash", "consequentHash" })
                {
                    if (body.TryGetProperty(field, out var hashProp) && hashProp.ValueKind == JsonValueKind.String)
                    {
                        FormulaToMemento[hashProp.GetString()!] = mementoCid;
                    }
                }
            }
        }
        Mementos[mementoCid] = envelope;
    }

    /// <summary>
    /// Check if antecedent → consequent is already proven in the pool.
    /// </summary>
    public JsonElement? VerifyImplication(string antecedentCid, string consequentCid)
    {
        foreach (var (_, envelope) in Mementos)
        {
            if (!envelope.TryGetProperty("evidence", out var evidence))
                continue;
            if (evidence.GetProperty("kind").GetString() != "implication")
                continue;
            if (!evidence.TryGetProperty("body", out var body))
                continue;
            var ant = body.TryGetProperty("antecedentHash", out var a) ? a.GetString() : null;
            var con = body.TryGetProperty("consequentHash", out var c) ? c.GetString() : null;
            if (ant == antecedentCid && con == consequentCid)
                return envelope;
        }
        return null;
    }

    /// <summary>
    /// Result of an implication check.
    /// </summary>
    public enum ImplicationResult
    {
        Unknown,
        ProvenDirect,
        ProvenTransitive,
        ProvenReflexive,
    }

    /// <summary>
    /// Check if antecedent → consequent holds via direct, transitive, or reflexive.
    /// </summary>
    public (ImplicationResult, List<string>?) CanImply(string antecedentCid, string consequentCid)
    {
        if (antecedentCid == consequentCid)
            return (ImplicationResult.ProvenReflexive, new List<string> { antecedentCid });

        if (VerifyImplication(antecedentCid, consequentCid).HasValue)
            return (ImplicationResult.ProvenDirect, new List<string> { antecedentCid, consequentCid });

        // Build graph and BFS for transitive
        var graph = new Dictionary<string, List<string>>();
        foreach (var (_, envelope) in Mementos)
        {
            if (!envelope.TryGetProperty("evidence", out var evidence))
                continue;
            if (evidence.GetProperty("kind").GetString() != "implication")
                continue;
            if (!evidence.TryGetProperty("body", out var body))
                continue;
            var ant = body.TryGetProperty("antecedentHash", out var a) ? a.GetString() : null;
            var con = body.TryGetProperty("consequentHash", out var c) ? c.GetString() : null;
            if (ant != null && con != null)
            {
                if (!graph.ContainsKey(ant))
                    graph[ant] = new List<string>();
                graph[ant].Add(con);
            }
        }

        var visited = new HashSet<string>();
        var queue = new Queue<List<string>>();
        queue.Enqueue(new List<string> { antecedentCid });

        while (queue.Count > 0)
        {
            var path = queue.Dequeue();
            var current = path[^1];

            if (!visited.Add(current))
                continue;

            if (graph.TryGetValue(current, out var neighbors))
            {
                foreach (var neighbor in neighbors)
                {
                    var newPath = new List<string>(path) { neighbor };
                    if (neighbor == consequentCid)
                        return (ImplicationResult.ProvenTransitive, newPath);
                    queue.Enqueue(newPath);
                }
            }
        }

        return (ImplicationResult.Unknown, null);
    }
}

public record LoadError(string ProofPath, string Reason);
