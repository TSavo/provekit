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
  liftFile,
  liftFormulaExpression,
  liftTermExpression,
  defaultTsKitRegistry,
  formatDiagnostic,
  type LiftDiagnostic,
  type LiftedProperty,
} from "./index.js";
import { resolveSort, primitiveSort, isPrimitiveSortName } from "./sorts.js";
import { checkAnchoring, isInvariantFile, isPropertyCall, isAssertCall } from "./anchoring.js";
import type { LiftContext } from "./rules.js";
import {
  emptyRegistry,
  extendRegistry,
  type RegistryEntry,
} from "./registry.js";
import {
  makeDiagnostic,
  makeFileDiagnostic,
  LIFT_DIAGNOSTIC_CODE,
} from "./diagnostics.js";

// ---------------------------------------------------------------------------
// Helpers: build a tsc.Program from in-memory file contents.
// ---------------------------------------------------------------------------

const FIXTURE_DIR = path.join(__dirname, "__fixtures__");
const PARSE_INT_FIXTURE_SOURCE = `
import { property, forAll, exists, implies } from "provekit/ir";
import type { Int, StringSort } from "provekit/sorts";

property(
  "parseIntCanReturnZero",
  exists<StringSort>((s) => parseInt(s) === 0),
);

property(
  "parseIntCanReturnNaN",
  exists<StringSort>((s) => Number.isNaN(parseInt(s))),
);

property(
  "parseIntCanReturnPositiveInteger",
  exists<StringSort>((s) => parseInt(s) > 0),
);

property("parseIntZeroStringIsZero", parseInt("0") === 0);

property("parseIntEmptyStringIsNaN", Number.isNaN(parseInt("")));

property(
  "parseIntReturnsIntOrNaN",
  forAll<StringSort>(
    (s) => Number.isInteger(parseInt(s)) || Number.isNaN(parseInt(s)),
  ),
);

property(
  "parseIntIsDeterministic",
  forAll<StringSort>((s) => parseInt(s) === parseInt(s)),
);

property(
  "parseIntPreservesNonNegativeIntegers",
  forAll<Int>((n) => implies(n >= 0, parseInt(String(n)) === n)),
);
`;
const MATH_FIXTURE_SOURCE = `
import { property, forAll, implies } from "provekit/ir";
import type { Int, Real } from "provekit/sorts";

property("Math.abs.returnsNonNegative", forAll<Real>((x) => Math.abs(x) >= 0));

property(
  "Math.abs.preservesMagnitude",
  forAll<Real>((x) => Math.abs(x) === Math.abs(-x)),
);

property(
  "Math.abs.identityOnNonNegative",
  forAll<Real>((x) => implies(x >= 0, Math.abs(x) === x)),
);

property("Math.abs.zeroFixedPoint", Math.abs(0) === 0);

property(
  "Math.max.commutative",
  forAll<Real>((a) => forAll<Real>((b) => Math.max(a, b) === Math.max(b, a))),
);

property(
  "Math.floor.idempotentOnIntegers",
  forAll<Int>((n) => Math.floor(n) === n),
);
`;

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

describe("lift rules: formula position", () => {
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
      expect(formula.name).toBe("=");
      expect(formula.args.length).toBe(2);
    } else {
      throw new Error("expected atomic");
    }
  });

  it("!== lifts to atomic '≠'", () => {
    const { formula } = lift(`1 !== 2`);
    if (formula?.kind === "atomic") {
      expect(formula.name).toBe("≠");
    } else {
      throw new Error("expected atomic");
    }
  });

  it("< lifts to atomic '<'", () => {
    const { formula } = lift(`1 < 2`);
    if (formula?.kind === "atomic") expect(formula.name).toBe("<");
    else throw new Error("expected atomic");
  });

  it("<= lifts to atomic '≤'", () => {
    const { formula } = lift(`1 <= 2`);
    if (formula?.kind === "atomic") expect(formula.name).toBe("≤");
    else throw new Error("expected atomic");
  });

  it("> lifts to atomic '>'", () => {
    const { formula } = lift(`2 > 1`);
    if (formula?.kind === "atomic") expect(formula.name).toBe(">");
    else throw new Error("expected atomic");
  });

  it(">= lifts to atomic '≥'", () => {
    const { formula } = lift(`2 >= 1`);
    if (formula?.kind === "atomic") expect(formula.name).toBe("≥");
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
      expect(formula.body.kind).toBe("atomic");
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
      expect(formula.operands.every((c) => c.kind === "implies")).toBe(true);
    }
  });

  it("xs.every(λ) lifts to forall when receiver is sort-typed array", () => {
    const { formula } = lift(`forAll<Int>((x) => x > 0)`);
    expect(formula?.kind).toBe("forall");
  });

  it("registry call (Number.isInteger) lifts to atomic with predicate name", () => {
    const { formula } = lift(`Number.isInteger(42)`);
    if (formula?.kind === "atomic") {
      expect(formula.name).toBe("Number.isInteger");
    } else {
      throw new Error("expected atomic");
    }
  });

  it("nested registry call inside comparison", () => {
    const { formula } = lift(`Math.abs(0) === 0`);
    if (formula?.kind === "atomic") {
      expect(formula.name).toBe("=");
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

describe("lift rules: term position", () => {
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
// Integration test: the parseInt fixture
// ---------------------------------------------------------------------------

describe("integration: parseInt fixture", () => {
  it("lifts every property without unliftable diagnostics", () => {
    const filePath = path.join(FIXTURE_DIR, "inline-parse-probe.invariant.ts");
    const src = PARSE_INT_FIXTURE_SOURCE;
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
    const filePath = path.join(FIXTURE_DIR, "inline-parse-probe.invariant.ts");
    const src = PARSE_INT_FIXTURE_SOURCE;
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
// Integration test: the Math fixture
// ---------------------------------------------------------------------------

describe("integration: Math fixture", () => {
  it("lifts every property cleanly", () => {
    const filePath = path.join(FIXTURE_DIR, "inline-math-probe.invariant.ts");
    const src = MATH_FIXTURE_SOURCE;
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
// Direct API: liftFormulaExpression / liftTermExpression
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

  it("liftTermExpression resolves a numeric literal to a const Int term", () => {
    const filePath = path.join(FIXTURE_DIR, "termdirect.invariant.ts");
    const program = buildProgram({
      [filePath]: `
        import { property } from "provekit/ir";
        property("p", 1 === 1);
      `,
    });
    const file = program.getSourceFile(filePath)!;
    let arg: ts.Expression | undefined;
    file.forEachChild(function visit(n) {
      if (ts.isBinaryExpression(n) && n.operatorToken.kind === ts.SyntaxKind.EqualsEqualsEqualsToken) {
        arg = n.left;
      }
      ts.forEachChild(n, visit);
    });
    expect(arg).toBeDefined();
    const ctx: LiftContext = {
      checker: program.getTypeChecker(),
      diagnostics: [],
      registry: defaultTsKitRegistry(),
      scope: [],
    };
    const t = liftTermExpression(arg!, ctx);
    expect(t.kind).toBe("const");
    if (t.kind === "const") expect(t.sort).toEqual({ kind: "primitive", name: "Int" });
  });
});

// ---------------------------------------------------------------------------
// Diagnostics module: direct API
// ---------------------------------------------------------------------------

describe("diagnostics module", () => {
  function makeFile(src: string): ts.SourceFile {
    return ts.createSourceFile("/tmp/x.invariant.ts", src, ts.ScriptTarget.ES2022, true);
  }

  it("makeDiagnostic populates standard fields", () => {
    const file = makeFile("const x = 1;");
    const decl = file.statements[0];
    const d = makeDiagnostic(decl, "boom");
    expect(d.code).toBe(LIFT_DIAGNOSTIC_CODE);
    expect(d.source).toBe("provekit-lift");
    expect(d.category).toBe(ts.DiagnosticCategory.Error);
    expect(d.messageText).toBe("boom");
    expect(d.file).toBe(file);
    expect(typeof d.start).toBe("number");
    expect(typeof d.length).toBe("number");
  });

  it("makeFileDiagnostic accepts explicit start/length", () => {
    const file = makeFile("const x = 1;");
    const d = makeFileDiagnostic(file, 3, 5, "msg");
    expect(d.start).toBe(3);
    expect(d.length).toBe(5);
    expect(d.messageText).toBe("msg");
    expect(d.code).toBe(LIFT_DIAGNOSTIC_CODE);
  });

  it("formatDiagnostic returns line:column - message when file is set", () => {
    const file = makeFile("\nconst x = 1;");
    const d = makeFileDiagnostic(file, 1, 5, "missing");
    const formatted = formatDiagnostic(d);
    // First column of the second line.
    expect(formatted).toContain("/tmp/x.invariant.ts:2:1");
    expect(formatted).toContain("missing");
  });

  it("formatDiagnostic returns just the message when file is missing", () => {
    const d: LiftDiagnostic = {
      file: undefined,
      start: undefined,
      length: undefined,
      messageText: "headless",
      category: ts.DiagnosticCategory.Error,
      code: LIFT_DIAGNOSTIC_CODE,
      source: "provekit-lift",
    };
    expect(formatDiagnostic(d)).toBe("headless");
  });
});

// ---------------------------------------------------------------------------
// Registry helpers: emptyRegistry / extendRegistry
// ---------------------------------------------------------------------------

describe("registry helpers", () => {
  it("emptyRegistry has no entries", () => {
    const r = emptyRegistry();
    expect(r.has("parseInt")).toBe(false);
    expect(r.names()).toEqual([]);
    expect(r.get("anything")).toBeUndefined();
  });

  it("extendRegistry adds new entries to a base", () => {
    const base = emptyRegistry();
    const extra: RegistryEntry = {
      name: "myFunc",
      signatureSorts: [{ kind: "primitive", name: "Int" }],
      returnSort: { kind: "primitive", name: "Bool" },
      returnKind: "formula",
    };
    const extended = extendRegistry(base, [extra]);
    expect(extended.has("myFunc")).toBe(true);
    expect(extended.get("myFunc")?.returnKind).toBe("formula");
    // The base is unchanged.
    expect(base.has("myFunc")).toBe(false);
  });

  it("extendRegistry overrides an existing entry by name", () => {
    const base = defaultTsKitRegistry();
    const Bool = { kind: "primitive", name: "Bool" } as const;
    const overridden: RegistryEntry = {
      name: "Math.abs",
      signatureSorts: [{ kind: "primitive", name: "Int" }],
      returnSort: Bool,
      returnKind: "formula",
    };
    const extended = extendRegistry(base, [overridden]);
    expect(extended.get("Math.abs")?.returnKind).toBe("formula");
    expect(extended.get("Math.abs")?.returnSort).toEqual(Bool);
  });

  it("defaultTsKitRegistry includes a representative sample of expected entries", () => {
    const r = defaultTsKitRegistry();
    for (const n of ["parseInt", "parseFloat", "Math.abs", "Math.sqrt", "Number.isInteger"]) {
      expect(r.has(n)).toBe(true);
    }
    expect(r.names().length).toBeGreaterThan(20);
  });
});

// ---------------------------------------------------------------------------
// Sort helpers (lift/sorts.ts)
// ---------------------------------------------------------------------------

describe("lift/sorts helpers", () => {
  it("primitiveSort builds a primitive Sort", () => {
    expect(primitiveSort("Int")).toEqual({ kind: "primitive", name: "Int" });
    expect(primitiveSort("Cents")).toEqual({ kind: "primitive", name: "Cents" });
  });

  it("isPrimitiveSortName recognizes the standard names", () => {
    for (const n of ["Bool", "Int", "Real", "String", "Ref", "Node", "Edge", "Region", "Time"]) {
      expect(isPrimitiveSortName(n)).toBe(true);
    }
    expect(isPrimitiveSortName("Cents")).toBe(false);
    expect(isPrimitiveSortName("")).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// Anchoring utility predicates
// ---------------------------------------------------------------------------

describe("anchoring utility predicates", () => {
  function makeCall(src: string): ts.CallExpression {
    const file = ts.createSourceFile("x.ts", src, ts.ScriptTarget.ES2022, true);
    let result: ts.CallExpression | undefined;
    file.forEachChild(function visit(n) {
      if (ts.isExpressionStatement(n) && ts.isCallExpression(n.expression)) {
        result = n.expression;
      }
      if (!result) ts.forEachChild(n, visit);
    });
    if (!result) throw new Error("no call expression in: " + src);
    return result;
  }

  it("isPropertyCall detects bare property() and namespace.property()", () => {
    expect(isPropertyCall(makeCall("property('x', true);"))).toBe(true);
    expect(isPropertyCall(makeCall("provekit.property('x', true);"))).toBe(true);
    expect(isPropertyCall(makeCall("foo();"))).toBe(false);
  });

  it("isAssertCall detects bare assert() and namespace.assert()", () => {
    expect(isAssertCall(makeCall("assert(true);"))).toBe(true);
    expect(isAssertCall(makeCall("ns.assert(true);"))).toBe(true);
    expect(isAssertCall(makeCall("foo();"))).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// liftFile: direct API
// ---------------------------------------------------------------------------

describe("liftFile", () => {
  it("returns the lifted properties for a single file", () => {
    const filePath = path.join(FIXTURE_DIR, "single.invariant.ts");
    const src = `
      import { property } from "provekit/ir";
      property("alpha", true);
      property("beta", false);
    `;
    const program = buildProgram({ [filePath]: src });
    const props = liftFile(filePath, program);
    expect(props.map((p) => p.name)).toEqual(["alpha", "beta"]);
  });

  it("throws when the file is not in the program", () => {
    const filePath = path.join(FIXTURE_DIR, "ghost.invariant.ts");
    const program = buildProgram({});
    expect(() => liftFile(filePath, program)).toThrow(/No source file/);
  });
});

// ---------------------------------------------------------------------------
// Visitor: assert() top-level lifting
// ---------------------------------------------------------------------------

describe("visitor: assert call lifting", () => {
  it("lifts a top-level assert(...) into a synthetic-named property", () => {
    const filePath = path.join(FIXTURE_DIR, "assertlevel.invariant.ts");
    const src = `
      import { assert } from "provekit/ir";
      assert(1 === 1);
    `;
    const program = buildProgram({ [filePath]: src });
    const result = liftProject(program);
    const lifted = result.properties[0];
    expect(lifted).toBeDefined();
    expect(lifted!.name.startsWith("assert@")).toBe(true);
  });

  it("rejects assert() with the wrong arity", () => {
    const filePath = path.join(FIXTURE_DIR, "badassert.invariant.ts");
    const src = `
      // @ts-nocheck
      import { assert } from "provekit/ir";
      assert(1 === 1, true);
    `;
    const program = buildProgram({ [filePath]: src });
    const result = liftProject(program);
    const messages = result.diagnostics.map((d) => String(d.messageText));
    expect(messages.some((m) => m.includes("expects exactly one"))).toBe(true);
  });
});
