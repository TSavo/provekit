// SPDX-License-Identifier: Apache-2.0
//
// First fixture, single Where. Empirical-first per the prompt: get this
// to byte-match BEFORE generating the rest of the test suite.
//
// We build the equivalent kit-authored contract via Provekit.IR and
// compare its MarshalDeclarations output against the lifter's IrJson.
// Byte-equality across the lifter and the kit is the only meaningful
// cross-impl conformance signal.

using System.IO;
using Provekit.Lift.Linq;
using Xunit;

using KitDecl = Provekit.IR.ContractDecl;
using KitFormula = Provekit.IR.Formula;
using KitAtomic = Provekit.IR.AtomicFormula;
using KitConn = Provekit.IR.ConnectiveFormula;
using KitQuant = Provekit.IR.QuantifierFormula;
using KitSort = Provekit.IR.Sort;
using KitTerm = Provekit.IR.Term;
using KitVar = Provekit.IR.VarTerm;
using KitConst = Provekit.IR.ConstTerm;
using KitConstVal = Provekit.IR.ConstValue;
using KitSerialize = Provekit.IR.Serialize;

public class SingleWhereTests
{
    [Fact]
    public void LiftsSingleWhereToOneMemento()
    {
        var src = File.ReadAllText("fixtures/single_where.cs");
        var lifter = new LinqLifter();
        var mementoes = lifter.Lift(src);

        Assert.Single(mementoes);
        var m = mementoes[0];
        Assert.Equal("positives", m.OutBinding);
        Assert.Equal(new[] { "xs" }, m.InputBindings);
    }

    [Fact]
    public void SingleWhereIrJsonByteEqualsKitAuthoredEquivalent()
    {
        var src = File.ReadAllText("fixtures/single_where.cs");
        var lifter = new LinqLifter();
        var mementoes = lifter.Lift(src);
        var liftedJson = mementoes[0].IrJson;

        // Construct the kit-authored equivalent. Lifter chose:
        //   bound var name: _x0
        //   sort: Ref (universe of xs)
        //   formula: forall _x0:Ref. (_x0 > 0) ⇒ member(_x0, positives)
        //   contract: { name: "positives_where", outBinding: "positives" }
        // After v1.5 Sort became an abstract record with sealed sub-records;
        // construct the Primitive variant directly instead of `new Sort(...)`.
        var sortRef = new KitSort.Primitive("Ref");
        var pred = new KitAtomic(">", new KitTerm[]
        {
            new KitVar("_x0"),
            // The literal `0` carries its own sort (Int) regardless of
            // the universe sort the quantifier ranges over.
            new KitConst(new KitConstVal.Int(0), KitSort.Int),
        });
        var memb = new KitAtomic("member", new KitTerm[]
        {
            new KitVar("_x0"),
            new KitVar("positives"),
        });
        KitFormula impl = new KitConn("implies", new KitFormula[] { pred, memb });
        KitFormula forall = new KitQuant("forall", "_x0", sortRef, impl);
        var kitContract = new KitDecl(
            Name: "positives_where",
            Pre: forall,
            Post: null,
            Inv: null,
            OutBinding: "positives");

        var kitJson = KitSerialize.MarshalDeclarations(new[] { kitContract });
        // The kit emits a Document (`[ ... ]`); the lifter emits a single
        // declaration. Wrap the lifted JSON in a one-element array for
        // an apples-to-apples comparison.
        var liftedAsDoc = "[" + liftedJson + "]";

        Assert.Equal(kitJson, liftedAsDoc);
    }
}
