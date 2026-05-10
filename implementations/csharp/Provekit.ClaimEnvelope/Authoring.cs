// SPDX-License-Identifier: Apache-2.0
//
// Authoring tag: typed union mirrored from the Rust/C++ kits. Lifted
// to a discriminated record hierarchy.

using V = Provekit.Canonicalizer.Value;

namespace Provekit.ClaimEnvelope;

public abstract record Authoring
{
    public abstract V ToValue();

    public sealed record KitAuthor(string Author, string? Note = null) : Authoring
    {
        public override V ToValue()
        {
            var entries = new List<KeyValuePair<string, V>>
            {
                new("producerKind", V.String("kit-author")),
                new("author", V.String(Author)),
            };
            if (!string.IsNullOrEmpty(Note))
            {
                entries.Add(new KeyValuePair<string, V>("note", V.String(Note!)));
            }
            return V.Object(entries);
        }
    }

    public sealed record Lift(string Lifter, string Evidence, string? SourceCid = null) : Authoring
    {
        public override V ToValue()
        {
            var entries = new List<KeyValuePair<string, V>>
            {
                new("producerKind", V.String("lift")),
                new("lifter", V.String(Lifter)),
                new("evidence", V.String(Evidence)),
            };
            if (!string.IsNullOrEmpty(SourceCid))
            {
                entries.Add(new KeyValuePair<string, V>("sourceCid", V.String(SourceCid!)));
            }
            return V.Object(entries);
        }
    }

    public sealed record Llm(
        string Model,
        string Version,
        string PromptCid,
        double Confidence,
        string? Rationale = null) : Authoring
    {
        public override V ToValue()
        {
            var entries = new List<KeyValuePair<string, V>>
            {
                new("producerKind", V.String("llm")),
                new("llm", V.String(Model)),
                new("llmVersion", V.String(Version)),
                new("promptCid", V.String(PromptCid)),
                // confidence carried as a milli-int (matches Rust peer's
                // (confidence * 1000) integer encoding, since the JCS
                // value tree is integer-only on the number side).
                new("confidence", V.Integer((long)(Confidence * 1000.0))),
            };
            if (!string.IsNullOrEmpty(Rationale))
            {
                entries.Add(new KeyValuePair<string, V>("rationale", V.String(Rationale!)));
            }
            return V.Object(entries);
        }
    }
}
