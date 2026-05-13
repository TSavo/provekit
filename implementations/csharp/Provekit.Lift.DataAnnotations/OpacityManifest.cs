// SPDX-License-Identifier: Apache-2.0
//
// OpacityManifest emission for vacuous-true DataAnnotations validators.
//
// Per protocol/specs/2026-05-02-opacity-manifest-grammar.md, an IR
// producer that emits a tractable placeholder for a position whose
// theory it cannot soundly translate MUST also record that position in
// an OpacityManifest. The DataAnnotations lift adapter sits in this
// slot for [EmailAddress], [Url], [Phone], [CreditCard]: the validator
// runs at runtime, but the IR emission is a kit predicate
// (`kit:<tag>`) with no provable content. The manifest names each
// opaque position by its content-address (BLAKE3-512 over the
// JCS-canonical IR-JSON of the Atomic node) and tags it with reason
// code `kit_predicate_no_semantics`.
//
// The library identity (System.ComponentModel.DataAnnotations + .NET
// runtime version) is pinned in the manifest's `compilerVersion`
// field; the Opacity record itself only carries (positionCid,
// reasonCode) per spec §2.

using Provekit.Canonicalizer;
using Provekit.IR;
using V = Provekit.Canonicalizer.Value;

namespace Provekit.Lift.DataAnnotations;

/// <summary>
/// One entry in an OpacityManifest. Field order in the JCS-canonical
/// emit form is determined by Jcs.Encode, not by record property order.
/// </summary>
public sealed record Opacity(string PositionCid, string ReasonCode);

/// <summary>
/// JCS-canonicalizable opacity manifest envelope per
/// protocol/specs/2026-05-02-opacity-manifest-grammar.md §2.
/// </summary>
public sealed record OpacityManifest(
    string Compiler,
    string CompilerVersion,
    IReadOnlyList<Opacity> Opacities,
    string ProtocolVersion);

public static class OpacityManifestBuilder
{
    /// <summary>
    /// Dialect identifier for this lift adapter, written to
    /// OpacityManifest.compiler. Distinct from the Go validator
    /// adapter's compiler name; manifests across language adapters are
    /// not byte-equivalent by design.
    /// </summary>
    public const string CompilerName = "provekit-lift-csharp-dataannotations";

    /// <summary>
    /// Adapter identity + the runtime validator surface this adapter
    /// targets. The "targets:" suffix names the BCL surface
    /// (System.ComponentModel.DataAnnotations) that ships the
    /// validation attributes the lift consumes. The suffix is
    /// documentary rather than a verifiable assembly pin: the lift
    /// reads attribute *types*, not the BCL's `Validator` runtime, and
    /// the BCL version is implicitly the TargetFramework's. A future
    /// revision MAY swap this for the actual
    /// `typeof(EmailAddressAttribute).Assembly.GetName().Version`
    /// to make the pin verifiable at build time.
    /// </summary>
    public const string CompilerVersion = "1.0.0+targets:System.ComponentModel.DataAnnotations";

    /// <summary>
    /// Per protocol/specs/2026-05-02-opacity-manifest-grammar.md §2.1,
    /// the manifest's `protocolVersion` field MUST be the literal
    /// "ir-compiler-protocol/2".
    /// </summary>
    public const string ProtocolVersion = "ir-compiler-protocol/2";

    /// <summary>
    /// The closed-enum reason code from spec §4 / §5 that fits every
    /// opacity entry this adapter ever emits: a kit-predicate Atomic
    /// the adapter has no theory semantics for.
    /// </summary>
    public const string KitPredicateNoSemantics = "kit_predicate_no_semantics";

    /// <summary>
    /// Build an OpacityManifest from a set of contract declarations.
    /// Scans every declaration's `Pre` formula for kit-predicate atoms
    /// (name with prefix "kit:") and emits one Opacity entry per
    /// distinct positionCid. Output ordering is JCS-canonical per
    /// spec §2.3 (positionCid ascending, then reasonCode).
    ///
    /// An empty input: or one with only sound predicates: produces a
    /// manifest with `Opacities = []` per spec §2.2: the envelope is
    /// mandatory even when no positions are opaque.
    /// </summary>
    public static OpacityManifest Build(IReadOnlyList<ContractDecl> decls)
    {
        var seen = new HashSet<string>();
        var entries = new List<Opacity>();

        foreach (var decl in decls)
        {
            if (decl.Pre is null) continue;
            CollectKitAtoms(decl.Pre, atom =>
            {
                var cid = ComputePositionCid(atom);
                var key = $"{cid}|{KitPredicateNoSemantics}";
                if (seen.Add(key))
                {
                    entries.Add(new Opacity(cid, KitPredicateNoSemantics));
                }
            });
        }

        // Spec §2.3: opacities sorted by positionCid ascending, then
        // reasonCode (lexicographic) as secondary key.
        entries.Sort((a, b) =>
        {
            var c = string.CompareOrdinal(a.PositionCid, b.PositionCid);
            return c != 0 ? c : string.CompareOrdinal(a.ReasonCode, b.ReasonCode);
        });

        return new OpacityManifest(
            Compiler: CompilerName,
            CompilerVersion: CompilerVersion,
            Opacities: entries,
            ProtocolVersion: ProtocolVersion);
    }

    /// <summary>
    /// Walk a formula tree and invoke <paramref name="onAtom"/> for
    /// every kit-predicate Atomic. Connectives (and / or / not /
    /// implies) and quantifiers (forall / exists / choice) descend
    /// into their children.
    /// </summary>
    private static void CollectKitAtoms(Formula f, Action<AtomicFormula> onAtom)
    {
        switch (f)
        {
            case AtomicFormula a:
                if (a.Name.StartsWith("kit:", StringComparison.Ordinal))
                {
                    onAtom(a);
                }
                return;
            case ConnectiveFormula c:
                foreach (var op in c.Operands) CollectKitAtoms(op, onAtom);
                return;
            case QuantifierFormula q:
                CollectKitAtoms(q.Body, onAtom);
                return;
            case ChoiceFormula ch:
                CollectKitAtoms(ch.Body, onAtom);
                return;
        }
    }

    /// <summary>
    /// positionCid = "blake3-512:" + lowercase-hex(BLAKE3-512(JCS(atom))).
    /// Per spec §3, the bytes hashed are the JCS-canonical form of the
    /// opaque IR subterm. We feed the AtomicFormula through
    /// Serialize.FormulaToValue → Jcs.Encode → Hash.Blake3_512Utf8.
    /// </summary>
    private static string ComputePositionCid(AtomicFormula atom)
    {
        V atomValue = Serialize.FormulaToValue(atom);
        var canonical = Jcs.Encode(atomValue);
        return Hash.Blake3_512Utf8(canonical);
    }

    /// <summary>
    /// Serialize an OpacityManifest to its JCS-canonical UTF-8 string
    /// form. The bytes are stable across runs of the same adapter on
    /// the same input. Suitable for writing to <c>&lt;cid&gt;.opacity.json</c>
    /// alongside the IR <c>.proof</c>.
    /// </summary>
    public static string ToJcs(OpacityManifest m)
    {
        var v = V.Object(
            ("compiler", V.String(m.Compiler)),
            ("compilerVersion", V.String(m.CompilerVersion)),
            ("opacities", V.Array(m.Opacities.Select(o => V.Object(
                ("positionCid", V.String(o.PositionCid)),
                ("reasonCode", V.String(o.ReasonCode))
            )).ToArray())),
            ("protocolVersion", V.String(m.ProtocolVersion))
        );
        return Jcs.Encode(v);
    }
}
