// SPDX-License-Identifier: Apache-2.0
//
// Lifter coverage for Where chains, All/Any, Count, Sum, GroupBy, query
// syntax, and mixed code. The single-Where byte-match conformance test
// lives in SingleWhereTests.cs (the empirical-first signal); this file
// asserts structural shape on the rest.

using System.IO;
using System.Linq;
using Provekit.Lift.Linq;
using Xunit;

public class LifterTests
{
    private static string Read(string fixture)
        => File.ReadAllText(Path.Combine("fixtures", fixture));

    [Fact]
    public void ChainedWhereProducesDagWithInputBindingsEdge()
    {
        var lifter = new LinqLifter();
        var ms = lifter.Lift(Read("chained_where.cs"));
        // Two LINQ statements -> two contract mementoes.
        Assert.Equal(2, ms.Count);

        // First memento: adults <- users
        var adults = ms.Single(m => m.OutBinding == "adults");
        Assert.Equal(new[] { "users" }, adults.InputBindings);

        // Second memento: voters <- adults
        var voters = ms.Single(m => m.OutBinding == "voters");
        Assert.Equal(new[] { "adults" }, voters.InputBindings);

        // The chain DAG primitive: voters' inputBindings names adults'
        // outBinding. The mint pipeline turns these into CID edges.
        Assert.Equal(adults.OutBinding, voters.InputBindings[0]);
    }

    [Fact]
    public void AllProducesSingleAtomicForall()
    {
        var lifter = new LinqLifter();
        var ms = lifter.Lift(Read("all_predicate.cs"));
        Assert.Single(ms);
        var m = ms[0];
        Assert.Equal("ok", m.OutBinding);
        // The IR-JSON should be a contract whose pre is a forall/atomic.
        Assert.Contains("\"kind\":\"forall\"", m.IrJson);
        Assert.Contains("\"name\":\"_x", m.IrJson);
        Assert.Contains("\"kind\":\"atomic\",\"name\":\">\"", m.IrJson);
        // No `member` predicate (All has no implicit-membership form).
        Assert.DoesNotContain("\"member\"", m.IrJson);
    }

    [Fact]
    public void GroupBySumApexProducesTwoMementoesAndDagEdge()
    {
        var lifter = new LinqLifter();
        var ms = lifter.Lift(Read("groupby_sum.cs"));
        Assert.Equal(2, ms.Count);

        var groups = ms.Single(m => m.OutBinding == "groups");
        Assert.Equal(new[] { "orders" }, groups.InputBindings);
        Assert.Contains("\"kind\":\"forall\"", groups.IrJson);
        Assert.Contains("\"kind\":\"exists\"", groups.IrJson);

        var totals = ms.Single(m => m.OutBinding == "totals");
        // totals' Select consumes groups; its `inputBindings` must
        // reference groups by binding-name so the chain-DAG resolves.
        Assert.Equal(new[] { "groups" }, totals.InputBindings);
        // The Select projection invokes Sum over each grouping; the
        // emitted IR must mention both `Sum` (as a ctor) and the inner
        // `Amount` member access.
        Assert.Contains("\"name\":\"Sum\"", totals.IrJson);
        Assert.Contains("\"name\":\"Amount\"", totals.IrJson);
    }

    [Fact]
    public void MixedCodeIgnoresNonLinq()
    {
        var lifter = new LinqLifter();
        var ms = lifter.Lift(Read("mixed_code.cs"));
        // Only the single Where call yields a memento; Console.WriteLine
        // and the foreach loop are not LINQ and are silently skipped.
        Assert.Single(ms);
        Assert.Equal("positives", ms[0].OutBinding);
    }

    [Fact]
    public void NullBoundaryWherePreservesNullLiteral()
    {
        var lifter = new LinqLifter();
        var ms = lifter.Lift(Read("null_boundary.cs"));

        Assert.Single(ms);
        var ir = ms[0].IrJson;
        Assert.Contains("\"kind\":\"atomic\",\"name\":\"≠\"", ir);
        Assert.Contains("\"kind\":\"const\",\"value\":null,\"sort\":{\"kind\":\"primitive\",\"name\":\"Ref\"}", ir);
    }

    [Fact]
    public void QuerySyntaxAndMethodSyntaxYieldEquivalentMementoes()
    {
        var lifter = new LinqLifter();
        var ms = lifter.Lift(Read("query_syntax.cs"));
        // Query form: `from x in xs where x > 0 select x` -- the
        // identity-projection Select is elided by Roslyn's query
        // translation, so this lowers to ONE Where call.
        // Method form: `xs.Where(x => x > 0).Select(x => x)` -- two
        // explicit invocations, two mementoes.
        // Total: 3 mementoes.
        Assert.Equal(3, ms.Count);

        // Both Where invocations (one from query form, one from method
        // form) should have structurally-identical predicate bodies.
        // Strip the bound-variable index AND the OutBinding-name slot
        // (which differs between the two forms because the LHS variable
        // is `positives` in one case, synthetic `result` in the chained
        // method-form case).
        var whereShapes = ms
            .Where(m => m.Name.EndsWith("_where"))
            .Select(m => StructuralShape(m.IrJson))
            .Distinct()
            .ToList();
        Assert.Single(whereShapes);
    }

    // Strips the trailing digit from `_xN` so two equivalent IR
    // emissions with different bound-var counters compare equal.
    private static string StripBoundVarIndex(MintedMemento m)
    {
        return System.Text.RegularExpressions.Regex.Replace(
            m.IrJson, @"_x\d+", "_x");
    }

    // Returns a normalised "shape" of the IR-JSON: bound vars collapsed
    // to `_x`, contract name + outBinding slot cleared, all member-of
    // collection-name references unified. Two LINQ forms that mean the
    // same thing must produce the same shape.
    private static string StructuralShape(string irJson)
    {
        var s = System.Text.RegularExpressions.Regex.Replace(irJson, @"_x\d+", "_x");
        // Erase contract name + outBinding (they encode binding scope,
        // not predicate shape).
        s = System.Text.RegularExpressions.Regex.Replace(
            s, "\"name\":\"[^\"]*_(where|select|all|any|count|sum|first|groupby|orderby)\"", "\"name\":\"<n>\"");
        s = System.Text.RegularExpressions.Regex.Replace(
            s, "\"outBinding\":\"[^\"]*\"", "\"outBinding\":\"<o>\"");
        // Erase the receiver/result variable references in `member`
        // predicates (they cite the LHS binding name, which differs).
        s = System.Text.RegularExpressions.Regex.Replace(
            s, "\"name\":\"member\",\"args\":\\[\\{\"kind\":\"var\",\"name\":\"_x\"\\},\\{\"kind\":\"var\",\"name\":\"[^\"]*\"\\}\\]",
            "\"name\":\"member\",\"args\":[{\"kind\":\"var\",\"name\":\"_x\"},{\"kind\":\"var\",\"name\":\"<v>\"}]");
        return s;
    }
}
