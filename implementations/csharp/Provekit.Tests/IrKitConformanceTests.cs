// SPDX-License-Identifier: Apache-2.0
//
// Cross-language conformance test for Provekit.IR (kit-authoring API).
//
// The fixture: build `x > 0` using the kit (Gt(Var("x"), Num(0))),
// serialize via FormulaToValue, then JCS-encode. The expected bytes
// are the v1.1.0 IR-JSON shape (NOT the canonicalizer fixture's shape
//: see CanonicalizerConformanceTests for that one).
//
// Both expected values were CAPTURED from the Rust peer's
// provekit-ir-symbolic crate running the equivalent program:
//
//   let f = gt(make_var("x"), num(0));
//   let v = formula_to_value(&f);
//   let bytes = encode_jcs(&v);
//   let h = blake3_512_of(bytes.as_bytes());
//
// If C# diverges from these values, EITHER our impl is wrong OR the
// Rust impl is wrong. Both are bugs.

using Provekit.Canonicalizer;
using Provekit.IR;
using Xunit;

using static Provekit.IR.Predicates;
using static Provekit.IR.Quantifiers;
using static Provekit.IR.Terms;
using static Provekit.IR.Collector;

namespace Provekit.Tests;

[Collection("CollectorSerial")]
public class IrKitConformanceTests
{
    private const string ExpectedKitJcsBytes =
        @"{""args"":[{""kind"":""var"",""name"":""x""}," +
        @"{""kind"":""const"",""sort"":{""kind"":""primitive"",""name"":""Int""},""value"":0}]," +
        @"""kind"":""atomic"",""name"":"">""}";

    private const string ExpectedKitPropertyHash =
        "blake3-512:" +
        "3e28aae830e80a6cfce0f9b2f54958e82c5a73e6fa2b6f9ebe3c6f2908422f09" +
        "13971045035c92ee4975a91479c98a4d5e0aeadedcfd74e37fd448ff22d95110";

    [Fact]
    public void Kit_GtVarXNum0_JcsBytes_MatchRustPeer()
    {
        // 1. Build AST via the kit
        var f = Gt(Var("x"), Num(0));
        // 2. Serialize to canonicalizer Value
        var v = Serialize.FormulaToValue(f);
        // 3. JCS-encode: keys re-sort at this layer
        var bytes = Jcs.Encode(v);
        Assert.Equal(ExpectedKitJcsBytes, bytes);
    }

    [Fact]
    public void Kit_GtVarXNum0_PropertyHash_MatchesRustPeer()
    {
        var f = Gt(Var("x"), Num(0));
        var v = Serialize.FormulaToValue(f);
        var h = Hash.Blake3_512(Jcs.EncodeUtf8(v));
        Assert.Equal(ExpectedKitPropertyHash, h);
    }

    [Fact]
    public void Quantifier_AutoNamesBoundVariables()
    {
        Quantifiers.ResetCounter();
        // forall x:Int. x > 0  --  bound name should be _x0
        var f = ForAll(Sort.Int, x => Gt(x, Num(0)));
        var q = Assert.IsType<QuantifierFormula>(f);
        Assert.Equal("forall", q.Kind);
        Assert.Equal("_x0", q.Name);
    }

    [Fact]
    public void Quantifier_NestedNamesIncrement()
    {
        Quantifiers.ResetCounter();
        var f = ForAll(Sort.Int, x =>
            Exists(Sort.Int, y => Gt(x, y)));
        var outer = Assert.IsType<QuantifierFormula>(f);
        Assert.Equal("_x0", outer.Name);
        var inner = Assert.IsType<QuantifierFormula>(outer.Body);
        Assert.Equal("exists", inner.Kind);
        Assert.Equal("_x1", inner.Name);
    }

    [Fact]
    public void Connective_UsesUnifiedOperandsArray()
    {
        // and / or / not / implies all use the same `operands` field: // that's the v1.1.0 maximal-uniformity property.
        var a = Gt(Var("x"), Num(0));
        var b = Lt(Var("x"), Num(10));
        var both = And(a, b);
        var c = Assert.IsType<ConnectiveFormula>(both);
        Assert.Equal("and", c.Kind);
        Assert.Equal(2, c.Operands.Count);

        var v = Serialize.FormulaToValue(both);
        var bytes = Jcs.Encode(v);
        // JCS sorts: kind, operands. Inside each atomic: args, kind, name.
        Assert.Contains("\"kind\":\"and\"", bytes);
        Assert.Contains("\"operands\":[", bytes);
    }

    [Fact]
    public void Collector_RoundTripsContract()
    {
        Collector.BeginCollecting();
        Must("parseInt", ForAll(Sort.Int, n => Gt(n, Num(0))));
        var decls = Collector.Finish();
        Assert.Single(decls);
        Assert.Equal("parseInt", decls[0].Name);
        Assert.Equal("out", decls[0].OutBinding);
        Assert.NotNull(decls[0].Pre);
        Assert.Null(decls[0].Post);
        Assert.Null(decls[0].Inv);
    }

    [Fact]
    public void Collector_ContractRejectsAllNull()
    {
        Collector.BeginCollecting();
        Assert.Throws<InvalidOperationException>(() =>
            Collector.Contract("empty"));
    }

    [Fact]
    public void Collector_MarshalDeclarations_InsertionOrder()
    {
        Collector.BeginCollecting();
        Must("parseInt", ForAll(Sort.Int, n => Gt(n, Num(0))));
        var decls = Collector.Finish();
        var s = Serialize.MarshalDeclarations(decls);
        // Insertion-order: kind, name, outBinding, then pre.
        Assert.Contains("\"kind\":\"contract\"", s);
        Assert.Contains("\"name\":\"parseInt\"", s);
        Assert.Contains("\"outBinding\":\"out\"", s);
        Assert.Contains("\"pre\":{\"kind\":\"forall\"", s);
    }

    [Fact]
    public void UnicodePredicates_RoundTripVerbatim_InKitJson()
    {
        // ≥, ≤, ≠ atomic predicate names. The kit's JCS output must
        // preserve their UTF-8 bytes.
        var f = Gte(Var("x"), Num(0));
        var v = Serialize.FormulaToValue(f);
        var bytes = Jcs.Encode(v);
        Assert.Contains("\"name\":\"≥\"", bytes);
    }
}
