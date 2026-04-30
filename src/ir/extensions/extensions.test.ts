/**
 * Extension authoring + registry tests.
 *
 * Each test resets the registry so cases are independent.
 */

import { describe, it, expect, beforeEach } from "vitest";
import {
  extensionSort,
  extensionPredicate,
  extensionCtor,
  registerExtensionDeclaration,
  resolveExtension,
  lookupSort,
  lookupPredicate,
  lookupCtor,
  listExtensions,
  _resetRegistry,
  UnresolvedExtensionError,
  ExtensionRegistrationError,
} from "./index.js";
import { num, str } from "../symbolic/primitives.js";
import { Int } from "../sorts.js";
import { forAll } from "../symbolic/property.js";

beforeEach(() => {
  _resetRegistry();
});

describe("extensionSort", () => {
  it("returns a Sort value with the declared name", () => {
    const FixedPoint8 = extensionSort({
      name: "FixedPoint8",
      semantics: [{ kind: "smt-lib-theory", theory: "FixedSizeBitVectors" }],
      compilers: ["smt-lib"],
    });
    expect(FixedPoint8).toEqual({ kind: "primitive", name: "FixedPoint8" });
  });

  it("registers the extension declaration in the registry", () => {
    extensionSort({
      name: "FixedPoint8",
      semantics: [{ kind: "smt-lib-theory", theory: "FixedSizeBitVectors" }],
      compilers: ["smt-lib"],
    });
    const decl = lookupSort("FixedPoint8");
    expect(decl).not.toBeNull();
    expect(decl!.introduces).toBe("sort");
    expect(decl!.compilers).toEqual(["smt-lib"]);
  });

  it("is idempotent for byte-identical re-registration", () => {
    const decl = {
      name: "FixedPoint8",
      semantics: [{ kind: "smt-lib-theory" as const, theory: "FixedSizeBitVectors" }],
      compilers: ["smt-lib"],
    };
    extensionSort(decl);
    expect(() => extensionSort(decl)).not.toThrow();
  });

  it("throws ExtensionRegistrationError on collision with a different body", () => {
    extensionSort({
      name: "FixedPoint8",
      semantics: [{ kind: "smt-lib-theory", theory: "FixedSizeBitVectors" }],
      compilers: ["smt-lib"],
    });
    expect(() =>
      extensionSort({
        name: "FixedPoint8",
        semantics: [{ kind: "natural-language", text: "different body" }],
        compilers: ["smt-lib"],
      }),
    ).toThrow(ExtensionRegistrationError);
  });
});

describe("extensionPredicate", () => {
  it("returns a function that builds atomic IrFormulas", () => {
    const isPrime = extensionPredicate({
      name: "is-prime",
      argSorts: ["Int"],
      semantics: [{ kind: "natural-language", text: "n > 1 and has no divisors other than 1 and n" }],
      compilers: ["smt-lib", "lean4"],
    });
    const formula = isPrime(num(7));
    expect(formula).toEqual({
      kind: "atomic",
      predicate: "is-prime",
      args: [{ kind: "const", value: 7, sort: Int }],
    });
  });

  it("registers the predicate declaration", () => {
    extensionPredicate({
      name: "is-prime",
      argSorts: ["Int"],
      semantics: [{ kind: "natural-language", text: "..." }],
      compilers: ["smt-lib"],
    });
    const decl = lookupPredicate("is-prime");
    expect(decl).not.toBeNull();
    expect(decl!.argSorts).toEqual(["Int"]);
  });
});

describe("extensionCtor", () => {
  it("returns a function that builds ctor IrTerms with the declared return sort", () => {
    const FixedPoint8 = extensionSort({
      name: "FixedPoint8",
      semantics: [{ kind: "smt-lib-theory", theory: "FixedSizeBitVectors" }],
      compilers: ["smt-lib"],
    });
    const fixedPointMul = extensionCtor({
      name: "fixed-point-mul",
      argSorts: [FixedPoint8, FixedPoint8],
      returnSort: FixedPoint8,
      semantics: [{ kind: "smt-lib-theory", theory: "FixedSizeBitVectors" }],
      compilers: ["smt-lib"],
    });
    const a = { kind: "var", name: "a", sort: FixedPoint8 } as const;
    const b = { kind: "var", name: "b", sort: FixedPoint8 } as const;
    const term = fixedPointMul(a, b);
    expect(term).toEqual({
      kind: "ctor",
      name: "fixed-point-mul",
      args: [a, b],
      sort: FixedPoint8,
    });
  });

  it("lifts JS primitives in args via liftToTerm", () => {
    const myCtor = extensionCtor({
      name: "myCtor",
      argSorts: ["Int", "String"],
      returnSort: "Int",
      semantics: [{ kind: "natural-language", text: "test" }],
      compilers: ["smt-lib"],
    });
    const result = myCtor(42, "hello");
    expect(result.kind).toBe("ctor");
    expect((result as { args: unknown[] }).args).toHaveLength(2);
  });
});

describe("resolveExtension", () => {
  it("returns the declaration when active compiler is in the list", () => {
    extensionSort({
      name: "FixedPoint8",
      semantics: [{ kind: "smt-lib-theory", theory: "FixedSizeBitVectors" }],
      compilers: ["smt-lib", "cvc5"],
    });
    const decl = resolveExtension("FixedPoint8", "sort", { activeCompiler: "smt-lib" });
    expect(decl.name).toBe("FixedPoint8");
  });

  it("throws UnresolvedExtensionError with reason 'no-declaration' when name is unknown", () => {
    expect(() => resolveExtension("Mystery", "sort", { activeCompiler: "smt-lib" })).toThrow(
      UnresolvedExtensionError,
    );
    try {
      resolveExtension("Mystery", "sort", { activeCompiler: "smt-lib" });
    } catch (e) {
      expect((e as UnresolvedExtensionError).reason).toBe("no-declaration");
    }
  });

  it("throws with reason 'compiler-incompatible' when active compiler is not in list", () => {
    extensionSort({
      name: "LeanOnlySort",
      semantics: [{ kind: "proof-assistant", system: "lean4", identifier: "Foo" }],
      compilers: ["lean4"],
    });
    expect(() =>
      resolveExtension("LeanOnlySort", "sort", { activeCompiler: "smt-lib" }),
    ).toThrow(UnresolvedExtensionError);
    try {
      resolveExtension("LeanOnlySort", "sort", { activeCompiler: "smt-lib" });
    } catch (e) {
      expect((e as UnresolvedExtensionError).reason).toBe("compiler-incompatible");
    }
  });

  it("throws when the registered declaration is the wrong kind", () => {
    extensionPredicate({
      name: "is-prime",
      argSorts: ["Int"],
      semantics: [{ kind: "natural-language", text: "..." }],
      compilers: ["smt-lib"],
    });
    expect(() =>
      resolveExtension("is-prime", "ctor", { activeCompiler: "smt-lib" }),
    ).toThrow(UnresolvedExtensionError);
  });
});

describe("listExtensions", () => {
  it("returns all registered declarations", () => {
    extensionSort({
      name: "S1",
      semantics: [{ kind: "natural-language", text: "..." }],
      compilers: ["smt-lib"],
    });
    extensionPredicate({
      name: "p1",
      argSorts: ["Int"],
      semantics: [{ kind: "natural-language", text: "..." }],
      compilers: ["smt-lib"],
    });
    expect(listExtensions()).toHaveLength(2);
  });

  it("returns empty after _resetRegistry", () => {
    extensionSort({
      name: "S1",
      semantics: [{ kind: "natural-language", text: "..." }],
      compilers: ["smt-lib"],
    });
    _resetRegistry();
    expect(listExtensions()).toHaveLength(0);
  });
});

describe("dogfood: authoring a fixed-point extension and using it end-to-end", () => {
  it("authors a sort + ctor + uses them in an IR formula", () => {
    // Author the new sort
    const FixedPoint8 = extensionSort({
      name: "FixedPoint8",
      semantics: [{ kind: "smt-lib-theory", theory: "FixedSizeBitVectors" }],
      compilers: ["smt-lib"],
    });

    // Author a multiplication ctor over it
    const fpMul = extensionCtor({
      name: "fp8-mul",
      argSorts: [FixedPoint8, FixedPoint8],
      returnSort: FixedPoint8,
      semantics: [{ kind: "smt-lib-theory", theory: "FixedSizeBitVectors" }],
      compilers: ["smt-lib"],
    });

    // Author an equality predicate (in real life this would just use the
    // built-in =, but this exercises the full extension shape)
    const fpEq = extensionPredicate({
      name: "fp8-eq",
      argSorts: [FixedPoint8, FixedPoint8],
      semantics: [{ kind: "smt-lib-theory", theory: "FixedSizeBitVectors" }],
      compilers: ["smt-lib"],
    });

    // Use them in an IR formula: forall x, y in FP8: fpEq(fpMul(x,y), fpMul(y,x))
    const formula = forAll(FixedPoint8, (x) =>
      forAll(FixedPoint8, (y) => fpEq(fpMul(x, y), fpMul(y, x))),
    );

    // The formula's structure references our extension names
    expect(formula.kind).toBe("forall");
    expect((formula as { sort: { name: string } }).sort.name).toBe("FixedPoint8");

    // All three declarations are in the registry
    expect(lookupSort("FixedPoint8")).not.toBeNull();
    expect(lookupCtor("fp8-mul")).not.toBeNull();
    expect(lookupPredicate("fp8-eq")).not.toBeNull();
  });
});
