/**
 * Tests for the TS-IR lifter.
 *
 * Strategy: build tsc.Program instances in-memory using
 * createCompilerHost overrides. Each test scopes a small set of files
 * (the fixture under test plus the SURFACE-API stubs) and asserts on
 * the LiftResult.
 */

import { describe, it, expect } from "vitest";
import path from "node:path";
import fs from "node:fs";
import ts from "typescript";

import {
  liftProject,
  liftFormulaExpression,
  liftTermExpression,
  defaultTsKitRegistry,
  type LiftDiagnostic,
  type LiftedProperty,
} from "./index.js";
import { resolveSort } from "./sorts.js";
import { checkAnchoring, isInvariantFile } from "./anchoring.js";
import type { LiftContext } from "./rules.js";

// ---------------------------------------------------------------------------
// Helpers — build a tsc.Program from in-memory file contents.
// ---------------------------------------------------------------------------

const FIXTURE_DIR = path.join(__dirname, "__fixtures__");

function buildProgram(files: Record<string, string>): ts.Program {
  const fileMap = new Map<string, string>();

  // Always inject the SURFACE API stubs.
  const stubPath = path.join(FIXTURE_DIR, "provekit-ir.d.ts");
  fileMap.set(stubPath, fs.readFileSync(stubPath, "utf8"));

  for (const [p, c] of Object.entries(files)) {
    fileMap.set(p, c);
  }

  const compilerOptions: ts.CompilerOptions = {
    target: ts.ScriptTarget.ES2022,
    module: ts.ModuleKind.ESNext,
    moduleResolution: ts.ModuleResolutionKind.Bundler,
    strict: true,
    skipLibCheck: true,
    noEmit: true,
    esModuleInterop: true,
  };

  const host = ts.createCompilerHost(compilerOptions, true);
  const realGetSourceFile = host.getSourceFile.bind(host);
  host.getSourceFile = (fileName, languageVersion, onError, shouldCreateNewSourceFile) => {
    if (fileMap.has(fileName)) {
      return ts.createSourceFile(fileName, fileMap.get(fileName)!, languageVersion, true);
    }
    return realGetSourceFile(fileName, languageVersion, onError, shouldCreateNewSourceFile);
  };
  const realFileExists = host.fileExists.bind(host);
  host.fileExists = (fn) => fileMap.has(fn) || realFileExists(fn);
  const realReadFile = host.readFile.bind(host);
  host.readFile = (fn) => fileMap.get(fn) ?? realReadFile(fn);

  return ts.createProgram({
    rootNames: Array.from(fileMap.keys()),
    options: compilerOptions,
    host,
  });
}

function fixtureFile(name: string): string {
  return path.join(FIXTURE_DIR, name);
}

function diagMessages(diags: LiftDiagnostic[]): string[] {
  return diags.map((d) => String(d.messageText));
}

function findProperty(props: LiftedProperty[], name: string): LiftedProperty | undefined {
  return props.find((p) => p.name === name);
}

// ---------------------------------------------------------------------------
// Anchoring (spec §3)
// ---------------------------------------------------------------------------

describe("anchoring", () => {
  it("detects property() in non-invariant file", () => {
    const src = `
      import { property } from "provekit/ir";
      property("bad", true);
    `;
    const filePath = path.join(FIXTURE_DIR, "billing/invoice.ts");
    const program = buildProgram({ [filePath]: src });
    const result = liftProject(program);
    const messages = diagMessages(result.diagnostics);
    expect(messages.some((m) => m.includes("may only appear in .invariant.ts"))).toBe(true);
  });

  it("allows property() in invariant file", () => {
    const src = `
      import { property } from "provekit/ir";
      property("ok", true);
    `;
    const filePath = path.join(FIXTURE_DIR, "billing/invoice.invariant.ts");
    const program = buildProgram({ [filePath]: src });
    const result = liftProject(program);
    const anchorMessages = diagMessages(result.diagnostics).filter((m) =>
      m.includes("may only appear in .invariant.ts"),
    );
    expect(anchorMessages).toEqual([]);
  });

  it("non-invariant file with no property() calls produces no anchoring diagnostics", () => {
    const filePath = path.join(FIXTURE_DIR, "billing/clean.ts");
    const program = buildProgram({
      [filePath]: `export const X = 1;\n`,
    });
    const result = liftProject(program);
    const anchorMessages = diagMessages(result.diagnostics).filter((m) =>
      m.includes("may only appear in .invariant.ts"),
    );
    expect(anchorMessages).toEqual([]);
  });

  it("isInvariantFile is endsWith-based", () => {
    expect(isInvariantFile("foo.invariant.ts")).toBe(true);
    expect(isInvariantFile("/abs/path/foo.invariant.ts")).toBe(true);
    expect(isInvariantFile("foo.ts")).toBe(false);
    expect(isInvariantFile("invariant.ts")).toBe(false);
  });

  it("checkAnchoring returns one diagnostic per property() call", () => {
    const src = `
      import { property } from "provekit/ir";
      property("a", true);
      property("b", false);
    `;
    const filePath = path.join(FIXTURE_DIR, "billing/two.ts");
    const program = buildProgram({ [filePath]: src });
    const file = program.getSourceFile(filePath)!;
    const diags = checkAnchoring(file);
    expect(diags.length).toBe(2);
  });
});

// ---------------------------------------------------------------------------
// Sort resolution (spec §5)
// ---------------------------------------------------------------------------

describe("sort resolution", () => {
  function sortFromAnnotation(annotation: string) {
    const filePath = path.join(FIXTURE_DIR, "sortprobe.invariant.ts");
    const src = `
      import { property, forAll } from "provekit/ir";
      import type { Int, Real, StringSort, Bool } from "provekit/sorts";
      type Cents = number & { readonly __sort: "Cents" };
      property("p", forAll<${annotation}>((x) => true));
    `;
    const program = buildProgram({ [filePath]: src });
    const result = liftProject(program);
    const lifted = result.properties.find((p) => p.name === "p");
    return { result, lifted };
  }

  it("resolves Int annotation", () => {
    const { lifted } = sortFromAnnotation("Int");
    expect(lifted?.formula.kind).toBe("forall");
    if (lifted?.formula.kind === "forall") {
      expect(lifted.formula.sort).toEqual({ kind: "primitive", name: "Int" });
    }
  });

  it("resolves Real annotation", () => {
    const { lifted } = sortFromAnnotation("Real");
    if (lifted?.formula.kind === "forall") {
      expect(lifted.formula.sort).toEqual({ kind: "primitive", name: "Real" });
    } else {
      throw new Error("expected forall");
    }
  });

  it("resolves user-defined branded sort (Cents)", () => {
    const { lifted } = sortFromAnnotation("Cents");
    if (lifted?.formula.kind === "forall") {
      expect(lifted.formula.sort).toEqual({ kind: "primitive", name: "Cents" });
    } else {
      throw new Error("expected forall");
    }
  });

  it("rejects unbranded number annotation", () => {
    const { result } = sortFromAnnotation("number");
    const messages = diagMessages(result.diagnostics);
    expect(messages.some((m) => m.includes("Cannot resolve sort"))).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// Per-AST-node lift rules (spec §9)
// ---------------------------------------------------------------------------

describe("lift rules — formula position", () => {
  function lift(srcExpr: string): { formula: import("../formulas.js").IrFormula | undefined; result: ReturnType<typeof liftProject> } {
    const filePath = path.join(FIXTURE_DIR, "exprprobe.invariant.ts");
    const src = `
      import { property, forAll, exists, implies, iff } from "provekit/ir";
      import type { Int, Real, StringSort } from "provekit/sorts";
      property("probe", ${srcExpr});
    `;
    const program = buildProgram({ [filePath]: src });
    const result = liftProject(program);
    const lifted = result.properties.find((p) => p.name === "probe");
    return { formula: lifted?.formula, result };
  }

  it("&& lifts to and", () => {
    const { formula } = lift(`true && false`);
    expect(formula?.kind).toBe("and");
  });

  it("|| lifts to or", () => {
    const { formula } = lift(`true || false`);
    expect(formula?.kind).toBe("or");
  });

  it("! lifts to not", () => {
    const { formula } = lift(`!true`);
    expect(formula?.kind).toBe("not");
  });

  it("=== lifts to atomic '='", () => {
    const { formula } = lift(`1 === 1`);
    if (formula?.kind === "atomic") {
      expect(formula.predicate).toBe("=");
      expect(formula.args.length).toBe(2);
    } else {
      throw new Error("expected atomic");
    }
  });

  it("!== lifts to atomic '≠'", () => {
    const { formula } = lift(`1 !== 2`);
    if (formula?.kind === "atomic") {
      expect(formula.predicate).toBe("≠");
    } else {
      throw new Error("expected atomic");
    }
  });

  it("< lifts to atomic '<'", () => {
    const { formula } = lift(`1 < 2`);
    if (formula?.kind === "atomic") expect(formula.predicate).toBe("<");
    else throw new Error("expected atomic");
  });

  it("<= lifts to atomic '≤'", () => {
    const { formula } = lift(`1 <= 2`);
    if (formula?.kind === "atomic") expect(formula.predicate).toBe("≤");
    else throw new Error("expected atomic");
  });

  it("> lifts to atomic '>'", () => {
    const { formula } = lift(`2 > 1`);
    if (formula?.kind === "atomic") expect(formula.predicate).toBe(">");
    else throw new Error("expected atomic");
  });

  it(">= lifts to atomic '≥'", () => {
    const { formula } = lift(`2 >= 1`);
    if (formula?.kind === "atomic") expect(formula.predicate).toBe("≥");
    else throw new Error("expected atomic");
  });

  it("ternary lifts to or-of-and (cond∧then ∨ ¬cond∧else)", () => {
    const { formula } = lift(`true ? false : true`);
    expect(formula?.kind).toBe("or");
  });

  it("forAll<T>(λ) lifts to forall with the right sort", () => {
    const { formula } = lift(`forAll<Int>((x) => x > 0)`);
    if (formula?.kind === "forall") {
      expect(formula.sort).toEqual({ kind: "primitive", name: "Int" });
      expect(formula.predicate.body.kind).toBe("atomic");
    } else {
      throw new Error("expected forall");
    }
  });

  it("exists<T>(λ) lifts to exists", () => {
    const { formula } = lift(`exists<StringSort>((s) => s === "")`);
    expect(formula?.kind).toBe("exists");
  });

  it("implies(a, b) lifts to implies", () => {
    const { formula } = lift(`implies(true, false)`);
    expect(formula?.kind).toBe("implies");
  });

  it("iff(a, b) lifts to and(implies(a,b), implies(b,a))", () => {
    const { formula } = lift(`iff(true, false)`);
    expect(formula?.kind).toBe("and");
    if (formula?.kind === "and") {
      expect(formula.conjuncts.every((c) => c.kind === "implies")).toBe(true);
    }
  });

  it("xs.every(λ) lifts to forall when receiver is sort-typed array", () => {
    const { formula } = lift(`forAll<Int>((x) => x > 0)`);
    expect(formula?.kind).toBe("forall");
  });

  it("registry call (Number.isInteger) lifts to atomic with predicate name", () => {
    const { formula } = lift(`Number.isInteger(42)`);
    if (formula?.kind === "atomic") {
      expect(formula.predicate).toBe("Number.isInteger");
    } else {
      throw new Error("expected atomic");
    }
  });

  it("nested registry call inside comparison", () => {
    const { formula } = lift(`Math.abs(0) === 0`);
    if (formula?.kind === "atomic") {
      expect(formula.predicate).toBe("=");
      expect(formula.args[0].kind).toBe("ctor");
      if (formula.args[0].kind === "ctor") {
        expect(formula.args[0].name).toBe("Math.abs");
      }
    } else {
      throw new Error("expected atomic");
    }
  });
});

// ---------------------------------------------------------------------------
// Term-position rules (arithmetic ctors, literals)
// ---------------------------------------------------------------------------

describe("lift rules — term position", () => {
  function liftCmp(srcExpr: string) {
    const filePath = path.join(FIXTURE_DIR, "termprobe.invariant.ts");
    const src = `
      import { property } from "provekit/ir";
      import type { Int } from "provekit/sorts";
      property("probe", ${srcExpr});
    `;
    const program = buildProgram({ [filePath]: src });
    const result = liftProject(program);
    return result.properties.find((p) => p.name === "probe")?.formula;
  }

  it("numeric literal lifts to const Int", () => {
    const f = liftCmp(`1 === 1`);
    if (f?.kind === "atomic") {
      const arg = f.args[0];
      expect(arg.kind).toBe("const");
      if (arg.kind === "const") {
        expect(arg.value).toBe(1);
        expect(arg.sort).toEqual({ kind: "primitive", name: "Int" });
      }
    }
  });

  it("a + b lifts to ctor '+'", () => {
    const f = liftCmp(`1 + 2 === 3`);
    if (f?.kind === "atomic") {
      const arg = f.args[0];
      expect(arg.kind).toBe("ctor");
      if (arg.kind === "ctor") expect(arg.name).toBe("+");
    }
  });

  it("a - b lifts to ctor '-'", () => {
    const f = liftCmp(`5 - 3 === 2`);
    if (f?.kind === "atomic") {
      const arg = f.args[0];
      if (arg.kind === "ctor") expect(arg.name).toBe("-");
      else throw new Error("expected ctor");
    }
  });

  it("unary - lifts to ctor 'negate'", () => {
    const f = liftCmp(`-1 === -1`);
    expect(f?.kind).toBe("atomic");
  });

  it("string literal lifts to const String", () => {
    const f = liftCmp(`"hello" === "hello"`);
    if (f?.kind === "atomic") {
      const arg = f.args[0];
      if (arg.kind === "const") {
        expect(arg.value).toBe("hello");
        expect(arg.sort).toEqual({ kind: "primitive", name: "String" });
      } else {
        throw new Error("expected const");
      }
    }
  });
});

// ---------------------------------------------------------------------------
// Negative cases (spec §6.2)
// ---------------------------------------------------------------------------

describe("rejection cases", () => {
  function liftBody(body: string) {
    const filePath = path.join(FIXTURE_DIR, "rejection.invariant.ts");
    const src = `
      import { property, forAll } from "provekit/ir";
      import type { Int } from "provekit/sorts";
      property("probe", ${body});
    `;
    const program = buildProgram({ [filePath]: src });
    return liftProject(program);
  }

  it("call to unregistered function rejected", () => {
    const result = liftBody(`somethingUnknown(1) === 0`);
    const messages = diagMessages(result.diagnostics);
    expect(messages.some((m) => m.includes("not in pure-function registry"))).toBe(true);
  });

  it("anchoring rejects property() in non-invariant file", () => {
    const filePath = path.join(FIXTURE_DIR, "leak/source.ts");
    const program = buildProgram({
      [filePath]: `
        import { property } from "provekit/ir";
        property("leaked", true);
      `,
    });
    const result = liftProject(program);
    expect(diagMessages(result.diagnostics).some((m) => m.includes(".invariant.ts"))).toBe(true);
  });

  it("destructuring quantifier param rejected", () => {
    const result = liftBody(`forAll<Int>(({ a }: any) => a > 0)`);
    const messages = diagMessages(result.diagnostics);
    expect(messages.some((m) => m.includes("destructuring"))).toBe(true);
  });

  it("block-bodied quantifier rejected", () => {
    const result = liftBody(`forAll<Int>((x) => { return x > 0; })`);
    const messages = diagMessages(result.diagnostics);
    expect(messages.some((m) => m.includes("must be an expression"))).toBe(true);
  });

  it("property() with non-string-literal name rejected", () => {
    const filePath = path.join(FIXTURE_DIR, "badname.invariant.ts");
    const program = buildProgram({
      [filePath]: `
        import { property } from "provekit/ir";
        const NAME = "x";
        property(NAME, true);
      `,
    });
    const result = liftProject(program);
    expect(
      diagMessages(result.diagnostics).some((m) =>
        m.includes("first argument must be a string literal"),
      ),
    ).toBe(true);
  });

  it("property() with wrong arity rejected", () => {
    const filePath = path.join(FIXTURE_DIR, "badarity.invariant.ts");
    const program = buildProgram({
      [filePath]: `
        import { property } from "provekit/ir";
        property("x" as any);
      `,
    });
    const result = liftProject(program);
    expect(
      diagMessages(result.diagnostics).some((m) => m.includes("expects exactly two")),
    ).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// Integration test — the parseInt fixture
// ---------------------------------------------------------------------------

describe("integration — parseInt fixture", () => {
  it("lifts every property without unliftable diagnostics", () => {
    const filePath = fixtureFile("parseInt.invariant.ts");
    const src = fs.readFileSync(filePath, "utf8");
    const program = buildProgram({ [filePath]: src });
    const result = liftProject(program);

    // Anchoring should be clean.
    const anchorMsgs = diagMessages(result.diagnostics).filter((m) =>
      m.includes("may only appear in .invariant.ts"),
    );
    expect(anchorMsgs).toEqual([]);

    // No unliftable diagnostics on the spine 8 properties.
    const unliftMsgs = diagMessages(result.diagnostics).filter(
      (m) => m.includes("not allowed") || m.includes("not in pure-function registry"),
    );
    expect(unliftMsgs).toEqual([]);

    // Expected named properties.
    const expected = [
      "parseIntCanReturnZero",
      "parseIntCanReturnNaN",
      "parseIntCanReturnPositiveInteger",
      "parseIntZeroStringIsZero",
      "parseIntEmptyStringIsNaN",
      "parseIntReturnsIntOrNaN",
      "parseIntIsDeterministic",
      "parseIntPreservesNonNegativeIntegers",
    ];
    for (const name of expected) {
      expect(findProperty(result.properties, name)?.name).toBe(name);
    }
    expect(result.properties.length).toBeGreaterThanOrEqual(expected.length);
  });

  it("each formula has a non-empty IR shape", () => {
    const filePath = fixtureFile("parseInt.invariant.ts");
    const src = fs.readFileSync(filePath, "utf8");
    const program = buildProgram({ [filePath]: src });
    const result = liftProject(program);
    for (const p of result.properties) {
      expect(p.formula).toBeDefined();
      expect(typeof p.formula.kind).toBe("string");
      expect(p.sourceLocation.filePath).toBe(filePath);
      expect(p.sourceLocation.line).toBeGreaterThan(0);
    }
  });
});

// ---------------------------------------------------------------------------
// Integration test — the Math fixture
// ---------------------------------------------------------------------------

describe("integration — Math fixture", () => {
  it("lifts every property cleanly", () => {
    const filePath = fixtureFile("Math.invariant.ts");
    const src = fs.readFileSync(filePath, "utf8");
    const program = buildProgram({ [filePath]: src });
    const result = liftProject(program);
    const unliftMsgs = diagMessages(result.diagnostics).filter(
      (m) => m.includes("not allowed") || m.includes("not in pure-function registry"),
    );
    expect(unliftMsgs).toEqual([]);
    expect(result.properties.length).toBeGreaterThanOrEqual(6);
  });

  it("Math.abs is registered", () => {
    const reg = defaultTsKitRegistry();
    expect(reg.has("Math.abs")).toBe(true);
    expect(reg.get("Math.abs")?.returnSort).toEqual({ kind: "primitive", name: "Real" });
  });

  it("parseInt is registered", () => {
    const reg = defaultTsKitRegistry();
    expect(reg.has("parseInt")).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// Direct API — liftFormulaExpression / liftTermExpression
// ---------------------------------------------------------------------------

describe("direct expression API", () => {
  it("liftFormulaExpression on a simple comparison", () => {
    const filePath = path.join(FIXTURE_DIR, "direct.invariant.ts");
    const program = buildProgram({
      [filePath]: `
        import { property } from "provekit/ir";
        property("p", 1 === 1);
      `,
    });
    const file = program.getSourceFile(filePath)!;
    // Walk to the property's formula expression.
    let formulaExpr: ts.Expression | undefined;
    file.forEachChild(function visit(n) {
      if (ts.isCallExpression(n) && ts.isIdentifier(n.expression) && n.expression.text === "property") {
        formulaExpr = n.arguments[1];
      }
      ts.forEachChild(n, visit);
    });
    expect(formulaExpr).toBeDefined();
    const ctx: LiftContext = {
      checker: program.getTypeChecker(),
      diagnostics: [],
      registry: defaultTsKitRegistry(),
      scope: [],
    };
    const f = liftFormulaExpression(formulaExpr!, ctx);
    expect(f.kind).toBe("atomic");
  });
});
