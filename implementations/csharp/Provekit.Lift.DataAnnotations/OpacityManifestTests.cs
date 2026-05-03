// SPDX-License-Identifier: Apache-2.0

using System.ComponentModel.DataAnnotations;
using Provekit.IR;
using Provekit.Lift.DataAnnotations;
using Xunit;

namespace Provekit.Lift.DataAnnotations.Tests;

// Test types isolating each vacuous-true validator. Reused across
// per-tag opacity emission tests below.
public class EmailOnly { [EmailAddress] public string V { get; set; } = ""; }
public class UrlOnly { [Url] public string V { get; set; } = ""; }
public class PhoneOnly { [Phone] public string V { get; set; } = ""; }
public class CreditCardOnly { [CreditCard] public string V { get; set; } = ""; }

public class Mixed
{
    [EmailAddress] public string Email { get; set; } = "";
    [Url] public string Site { get; set; } = "";
}

public class TwoEmails
{
    [EmailAddress] public string Primary { get; set; } = "";
    [EmailAddress] public string Secondary { get; set; } = "";
}

public class ManyOpaques
{
    [EmailAddress] public string A { get; set; } = "";
    [Url] public string B { get; set; } = "";
    [Phone] public string C { get; set; } = "";
    [CreditCard] public string D { get; set; } = "";
}

public class RangeOnly
{
    [Range(0, 100)] public int V { get; set; }
}

public class RequiredEmail
{
    [Required] [EmailAddress] public string V { get; set; } = "";
}

public class OpacityManifestTests
{
    // Spec §2.2: every conformant adapter MUST emit a manifest envelope
    // even when no positions are opaque.
    [Fact]
    public void Build_EmptyDeclarations_EmitsEnvelope()
    {
        var m = OpacityManifestBuilder.Build(Array.Empty<ContractDecl>());
        Assert.Equal("ir-compiler-protocol/2", m.ProtocolVersion);
        Assert.Equal("provekit-lift-csharp-dataannotations", m.Compiler);
        Assert.False(string.IsNullOrEmpty(m.CompilerVersion));
        Assert.Contains("System.ComponentModel.DataAnnotations", m.CompilerVersion);
        Assert.Contains("targets:", m.CompilerVersion);
        Assert.Empty(m.Opacities);
    }

    // Sound predicates (no kit:* atoms) emit empty opacities.
    [Fact]
    public void Build_NoVacuousPredicates_EmitsEmptyOpacities()
    {
        var decls = DataAnnotationsLift.LiftType<RangeOnly>();
        var m = OpacityManifestBuilder.Build(decls);
        Assert.Empty(m.Opacities);
    }

    [Theory]
    [InlineData(typeof(EmailOnly))]
    [InlineData(typeof(UrlOnly))]
    [InlineData(typeof(PhoneOnly))]
    [InlineData(typeof(CreditCardOnly))]
    public void Build_VacuousValidator_EmitsOneOpacity(Type t)
    {
        var decls = DataAnnotationsLift.LiftType(t);
        var m = OpacityManifestBuilder.Build(decls);
        Assert.Single(m.Opacities);
        Assert.Equal("kit_predicate_no_semantics", m.Opacities[0].ReasonCode);
        Assert.StartsWith("blake3-512:", m.Opacities[0].PositionCid);
    }

    // Pre-task collision: both [EmailAddress] and [Url] lifted to
    // Predicates.And() (empty conjunction), giving byte-identical IR
    // and a single positionCid for two distinct semantic positions.
    // This test guards against regression.
    [Fact]
    public void Build_DistinctValidatorsOnDifferentFields_DistinctPositions()
    {
        var decls = DataAnnotationsLift.LiftType<Mixed>();
        var m = OpacityManifestBuilder.Build(decls);
        Assert.Equal(2, m.Opacities.Count);
        Assert.NotEqual(m.Opacities[0].PositionCid, m.Opacities[1].PositionCid);
    }

    // Same validator, different fields ⇒ distinct positionCids: the
    // Atomic node's args include the var name (Primary vs Secondary).
    [Fact]
    public void Build_SameValidatorOnTwoFields_DistinctPositions()
    {
        var decls = DataAnnotationsLift.LiftType<TwoEmails>();
        var m = OpacityManifestBuilder.Build(decls);
        Assert.Equal(2, m.Opacities.Count);
        Assert.NotEqual(m.Opacities[0].PositionCid, m.Opacities[1].PositionCid);
    }

    // Spec §2.3: opacities sorted ascending by positionCid.
    [Fact]
    public void Build_OpacitiesSortedAscending()
    {
        var decls = DataAnnotationsLift.LiftType<ManyOpaques>();
        var m = OpacityManifestBuilder.Build(decls);
        Assert.Equal(4, m.Opacities.Count);
        for (var i = 1; i < m.Opacities.Count; i++)
        {
            Assert.True(
                string.CompareOrdinal(m.Opacities[i - 1].PositionCid, m.Opacities[i].PositionCid) <= 0,
                $"opacities not sorted at i={i}: {m.Opacities[i - 1].PositionCid} > {m.Opacities[i].PositionCid}");
        }
    }

    // [Required] [EmailAddress] together emits and(neq, kit:email);
    // only the email atom is opaque, so Opacities.Count == 1.
    [Fact]
    public void Build_RequiredAndEmail_OnlyEmailIsOpaque()
    {
        var decls = DataAnnotationsLift.LiftType<RequiredEmail>();
        var m = OpacityManifestBuilder.Build(decls);
        Assert.Single(m.Opacities);
    }

    // ToJcs returns a string whose byte form contains the spec's
    // required keys and is idempotent (re-parsing + re-encoding gives
    // identical bytes).
    [Fact]
    public void ToJcs_ContainsRequiredKeys_AndIsIdempotent()
    {
        var decls = DataAnnotationsLift.LiftType<EmailOnly>();
        var m = OpacityManifestBuilder.Build(decls);
        var jcs = OpacityManifestBuilder.ToJcs(m);

        Assert.Contains("\"protocolVersion\":\"ir-compiler-protocol/2\"", jcs);
        Assert.Contains("\"compiler\":\"provekit-lift-csharp-dataannotations\"", jcs);
        Assert.Contains("\"opacities\":", jcs);
        Assert.Contains("\"reasonCode\":\"kit_predicate_no_semantics\"", jcs);

        // Idempotence: same manifest re-encoded.
        var jcs2 = OpacityManifestBuilder.ToJcs(m);
        Assert.Equal(jcs, jcs2);
    }

    // Spec sanity: VacuousKitPredicates exposes the four task-spec'd
    // names.
    [Fact]
    public void VacuousKitPredicates_CoversTaskSpec()
    {
        var preds = DataAnnotationsLift.VacuousKitPredicates;
        Assert.Contains("kit:email", preds);
        Assert.Contains("kit:url", preds);
        Assert.Contains("kit:phone", preds);
        Assert.Contains("kit:credit_card", preds);
    }

    // The lift's IR emission must use the kit predicate (NOT the empty
    // conjunction). Guards against regression to the pre-task
    // placeholder of Predicates.And().
    [Fact]
    public void EmailLift_EmitsKitAtomic_NotEmptyAnd()
    {
        var decls = DataAnnotationsLift.LiftType<EmailOnly>();
        var jcs = Serialize.MarshalDeclarations(decls);
        Assert.Contains("\"kind\":\"atomic\",\"name\":\"kit:email\"", jcs);
        Assert.DoesNotContain("\"operands\":[]", jcs);
    }

    // Cross-language byte-conformance pin per
    // protocol/specs/2026-05-02-opacity-manifest-grammar.md §6.
    //
    // Both the Go validator lift (`V string `validate:"email"``) and
    // this C# DataAnnotations lift (`[EmailAddress] public string V`)
    // lift to the byte-identical IR atom:
    //
    //   {"args":[{"kind":"var","name":"V"}],"kind":"atomic","name":"kit:email"}
    //
    // The BLAKE3-512 of the JCS-canonical bytes — the positionCid —
    // MUST be identical across languages. This test pins the hash;
    // the Go peer test in opacity_manifest_test.go asserts the same
    // constant.
    public const string KitEmailPositionCidPin =
        "blake3-512:ea31bf7d7052172f05c3254fc2cfb8809daf9f4a9578090ce7c46b35ab5f1d208c16e58a98314a8659dfcae1858165771eafa8639e7522ff2870140933a7cd27";

    [Fact]
    public void Build_KitEmail_GoldenPositionCid_CrossLanguagePin()
    {
        var decls = DataAnnotationsLift.LiftType<EmailOnly>();
        var m = OpacityManifestBuilder.Build(decls);
        Assert.Single(m.Opacities);
        Assert.Equal(KitEmailPositionCidPin, m.Opacities[0].PositionCid);
    }
}
