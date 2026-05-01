// Demonstration assertions for the report. Each test asserts the IR
// shape of the apex examples. They double as living docs.

using System.IO;
using System.Linq;
using Provekit.Lift.Linq;
using Xunit;

public class DemoReportTests
{
    [Fact]
    public void ChainedWhereDagRendersExpectedShape()
    {
        var lifter = new LinqLifter();
        var ms = lifter.Lift(File.ReadAllText("fixtures/chained_where.cs"));
        // adults <- users
        Assert.Equal("users", ms[0].InputBindings.Single());
        Assert.Equal("adults", ms[0].OutBinding);
        // voters <- adults  -- the inputCids edge in the chain DAG.
        Assert.Equal("adults", ms[1].InputBindings.Single());
        Assert.Equal("voters", ms[1].OutBinding);
    }

    [Fact]
    public void GroupBySumApexProducesIntegratedDag()
    {
        var lifter = new LinqLifter();
        var ms = lifter.Lift(File.ReadAllText("fixtures/groupby_sum.cs"));
        Assert.Equal(2, ms.Count);
        // groups <- orders, totals <- groups: chain DAG over GroupBy + Select(g.Sum).
        Assert.Equal("orders", ms[0].InputBindings.Single());
        Assert.Equal("groups", ms[0].OutBinding);
        Assert.Equal("groups", ms[1].InputBindings.Single());
        Assert.Equal("totals", ms[1].OutBinding);
    }
}
