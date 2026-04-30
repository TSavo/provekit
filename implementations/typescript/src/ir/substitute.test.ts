import { describe, it, expect } from "vitest";
import { instantiateOutermostForall, SubstituteError } from "./substitute.js";
import type { IrFormula, IrTerm } from "./formulas.js";

const StringSort = { kind: "primitive" as const, name: "String" };
const IntSort = { kind: "primitive" as const, name: "Int" };

describe("instantiateOutermostForall", () => {
  it("substitutes a Const into the body of a forall", () => {
    // forall s: String. nonempty(s)
    const formula: IrFormula = {
      kind: "forall",
      sort: StringSort,
      predicate: {
        kind: "lambda",
        varName: "s",
        sort: StringSort,
        body: {
          kind: "atomic",
          predicate: "nonempty",
          args: [{ kind: "var", name: "s", sort: StringSort }],
        },
      },
    };
    const term: IrTerm = { kind: "const", value: "hello", sort: StringSort };
    const result = instantiateOutermostForall(formula, term);
    expect(result).toEqual({
      kind: "atomic",
      predicate: "nonempty",
      args: [{ kind: "const", value: "hello", sort: StringSort }],
    });
  });

  it("preserves untouched conjuncts and substitutes only matching var refs", () => {
    // forall x: Int. (x > 0) ∧ (positive(other))
    const formula: IrFormula = {
      kind: "forall",
      sort: IntSort,
      predicate: {
        kind: "lambda",
        varName: "x",
        sort: IntSort,
        body: {
          kind: "and",
          conjuncts: [
            {
              kind: "atomic",
              predicate: ">",
              args: [
                { kind: "var", name: "x", sort: IntSort },
                { kind: "const", value: 0, sort: IntSort },
              ],
            },
            {
              kind: "atomic",
              predicate: "positive",
              args: [{ kind: "var", name: "other", sort: IntSort }],
            },
          ],
        },
      },
    };
    const term: IrTerm = { kind: "const", value: 5, sort: IntSort };
    const result = instantiateOutermostForall(formula, term);
    expect(result.kind).toBe("and");
    if (result.kind !== "and") throw new Error();
    expect(result.conjuncts[0]).toEqual({
      kind: "atomic",
      predicate: ">",
      args: [
        { kind: "const", value: 5, sort: IntSort },
        { kind: "const", value: 0, sort: IntSort },
      ],
    });
    expect(result.conjuncts[1]).toEqual({
      kind: "atomic",
      predicate: "positive",
      args: [{ kind: "var", name: "other", sort: IntSort }],
    });
  });

  it("substitutes inside ctor terms", () => {
    // forall s: String. nonempty(parseInt(s))
    const formula: IrFormula = {
      kind: "forall",
      sort: StringSort,
      predicate: {
        kind: "lambda",
        varName: "s",
        sort: StringSort,
        body: {
          kind: "atomic",
          predicate: "nonempty",
          args: [
            {
              kind: "ctor",
              name: "parseInt",
              args: [{ kind: "var", name: "s", sort: StringSort }],
              sort: IntSort,
            },
          ],
        },
      },
    };
    const term: IrTerm = { kind: "const", value: "42", sort: StringSort };
    const result = instantiateOutermostForall(formula, term);
    expect(result).toEqual({
      kind: "atomic",
      predicate: "nonempty",
      args: [
        {
          kind: "ctor",
          name: "parseInt",
          args: [{ kind: "const", value: "42", sort: StringSort }],
          sort: IntSort,
        },
      ],
    });
  });

  it("does not substitute past an inner shadowing binder", () => {
    // forall s: String. exists s: String. nonempty(s)
    // The outer s should NOT substitute into the body of the inner exists
    // because the inner s is a different variable that happens to share the name.
    const formula: IrFormula = {
      kind: "forall",
      sort: StringSort,
      predicate: {
        kind: "lambda",
        varName: "s",
        sort: StringSort,
        body: {
          kind: "exists",
          sort: StringSort,
          predicate: {
            kind: "lambda",
            varName: "s",
            sort: StringSort,
            body: {
              kind: "atomic",
              predicate: "nonempty",
              args: [{ kind: "var", name: "s", sort: StringSort }],
            },
          },
        },
      },
    };
    const term: IrTerm = { kind: "const", value: "X", sort: StringSort };
    const result = instantiateOutermostForall(formula, term);
    // The inner s is unaffected.
    expect(result).toEqual({
      kind: "exists",
      sort: StringSort,
      predicate: {
        kind: "lambda",
        varName: "s",
        sort: StringSort,
        body: {
          kind: "atomic",
          predicate: "nonempty",
          args: [{ kind: "var", name: "s", sort: StringSort }],
        },
      },
    });
  });

  it("throws when input is not a forall", () => {
    const notAForall: IrFormula = {
      kind: "atomic",
      predicate: "true",
      args: [],
    };
    expect(() =>
      instantiateOutermostForall(notAForall, { kind: "const", value: 1, sort: IntSort }),
    ).toThrow(SubstituteError);
  });

  it("throws on capture: free var in term collides with inner binding", () => {
    // forall s. forall t. nonempty(s, t)  where we substitute s with var "t"
    const formula: IrFormula = {
      kind: "forall",
      sort: StringSort,
      predicate: {
        kind: "lambda",
        varName: "s",
        sort: StringSort,
        body: {
          kind: "forall",
          sort: StringSort,
          predicate: {
            kind: "lambda",
            varName: "t",
            sort: StringSort,
            body: {
              kind: "atomic",
              predicate: "rel",
              args: [
                { kind: "var", name: "s", sort: StringSort },
                { kind: "var", name: "t", sort: StringSort },
              ],
            },
          },
        },
      },
    };
    const collidingTerm: IrTerm = { kind: "var", name: "t", sort: StringSort };
    expect(() => instantiateOutermostForall(formula, collidingTerm)).toThrow(
      /capture/,
    );
  });
});
